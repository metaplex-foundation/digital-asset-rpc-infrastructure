use {
    crate::{
        error::IngesterError,
        events::handle_event,
        get_gummy_roll_events,
        parsers::{InstructionBundle, ProgramHandler, ProgramHandlerConfig},
        save_changelog_events,
        tasks::BgTask,
        utils::bytes_from_fb_table,
        utils::filter_events_from_logs,
    },
    anchor_client::anchor_lang::{self, prelude::Pubkey, AnchorDeserialize},
    async_trait::async_trait,
    bubblegum::state::{
        leaf_schema::{LeafSchema, LeafSchemaEvent, Version},
        NFTDecompressionEvent,
    },
    digital_asset_types::{
        adapter::{TokenStandard, UseMethod, Uses},
        dao::{
            asset, asset_authority, asset_creators, asset_data, asset_grouping,
            sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
        },
        json::ChainDataV1,
    },
    flatbuffers::{ForwardsUOffset, Vector},
    lazy_static::lazy_static,
    num_traits::FromPrimitive,
    plerkle_serialization::transaction_info_generated::transaction_info::{self},
    sea_orm::{
        entity::*, query::*, sea_query::OnConflict, DatabaseConnection, DatabaseTransaction,
        DbBackend, DbErr, JsonValue, SqlxPostgresConnector, TransactionTrait,
    },
    serde_json, solana_sdk,
    solana_sdk::pubkeys,
    sqlx::{self, Pool, Postgres},
    std::fmt::{Display, Formatter},
    tokio::sync::mpsc::UnboundedSender,
};

pubkeys!(
    BubblegumProgramID,
    "BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY"
);

pub struct BubblegumHandler {
    id: Pubkey,
    storage: DatabaseConnection,
    task_sender: UnboundedSender<Box<dyn BgTask>>,
}

pub struct DownloadMetadata {
    asset_data_id: i64,
    uri: String,
}

impl Display for DownloadMetadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DownloadMetadata from {} for {}",
            self.uri, self.asset_data_id
        )
    }
}

#[async_trait]
impl BgTask for DownloadMetadata {
    async fn task(&self, db: &DatabaseConnection) -> Result<(), IngesterError> {
        let body: serde_json::Value = reqwest::get(self.uri.clone()) // Need to check for malicious sites ?
            .await?
            .json()
            .await?;
        let model = asset_data::ActiveModel {
            id: Unchanged(self.asset_data_id),
            metadata: Set(body),
            ..Default::default()
        };
        asset_data::Entity::update(model)
            .filter(asset_data::Column::Id.eq(self.asset_data_id))
            .exec(db)
            .await
            .map(|_| ())
            .map_err(|db| {
                IngesterError::TaskManagerError(format!(
                    "Database error with {}, error: {}",
                    self, db
                ))
            })
    }
}

#[async_trait]
impl ProgramHandler for BubblegumHandler {
    fn id(&self) -> Pubkey {
        self.id
    }

    fn config(&self) -> &ProgramHandlerConfig {
        lazy_static! {
            static ref CONFIG: ProgramHandlerConfig = ProgramHandlerConfig {
                responds_to_instruction: true,
                responds_to_account: false
            };
        }
        return &CONFIG;
    }

    async fn handle_instruction(&self, bundle: &InstructionBundle) -> Result<(), IngesterError> {
        handle_bubblegum_instruction(
            &bundle.instruction,
            bundle.slot,
            &bundle.instruction_logs,
            &bundle.keys,
            &self.storage,
            &self.task_sender,
        )
        .await
    }
}

impl BubblegumHandler {
    pub fn new(pool: Pool<Postgres>, task_queue: UnboundedSender<Box<dyn BgTask>>) -> Self {
        BubblegumHandler {
            id: BubblegumProgramID(),
            task_sender: task_queue,
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
        }
    }
}

fn get_leaf_event(logs: &Vec<&str>) -> Result<LeafSchemaEvent, IngesterError> {
    get_bubblegum_event(logs)
}

fn get_decompress_event(logs: &Vec<&str>) -> Result<NFTDecompressionEvent, IngesterError> {
    get_bubblegum_event(logs)
}

fn get_bubblegum_event<T: anchor_lang::Event + anchor_lang::AnchorDeserialize>(
    logs: &Vec<&str>,
) -> Result<T, IngesterError> {
    let event_logs = filter_events_from_logs(logs);
    if event_logs.is_err() {
        println!("Error finding event logs in bubblegum logs");
        return Err(IngesterError::CompressedAssetEventMalformed);
    }

    let mut found_event: Option<T> = None;
    for event in event_logs.unwrap() {
        let event = handle_event::<T>(event);
        if event.is_ok() {
            found_event = event.ok()
        }
    }
    found_event.ok_or(IngesterError::CompressedAssetEventMalformed)
}

async fn tree_change_only<'a>(
    db: &DatabaseConnection,
    slot: u64,
    logs: &Vec<&'a str>,
) -> Result<(), IngesterError> {
    let gummy_roll_events = get_gummy_roll_events(logs)?;
    db.transaction::<_, _, IngesterError>(|txn| {
        Box::pin(async move {
            save_changelog_events(gummy_roll_events, slot, txn)
                .await
                .map(|_| ())
        })
    })
    .await
    .map_err(Into::into)
}

async fn update_asset(
    txn: &DatabaseTransaction,
    id: Vec<u8>,
    seq: Option<u64>,
    model: asset::ActiveModel,
) -> Result<(), IngesterError> {
    let update_one = if let Some(seq) = seq {
        asset::Entity::update(model)
            .filter(asset::Column::Id.eq(id))
            .filter(asset::Column::Seq.lte(seq))
    } else {
        asset::Entity::update(model).filter(asset::Column::Id.eq(id))
    };

    match update_one.exec(txn).await {
        Ok(_) => Ok(()),
        Err(err) => match err {
            DbErr::RecordNotFound(ref s) => {
                if s.find("None of the database rows are affected") != None {
                    Ok(())
                } else {
                    Err(IngesterError::from(err))
                }
            }
            _ => Err(IngesterError::from(err)),
        },
    }
}

async fn handle_bubblegum_instruction<'a, 'b, 't>(
    instruction: &'a transaction_info::CompiledInstruction<'a>,
    slot: u64,
    logs: &Vec<&'a str>,
    keys: &Vector<'b, ForwardsUOffset<transaction_info::Pubkey<'b>>>,
    db: &DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let ix_type = bubblegum::get_instruction_type(instruction.data().unwrap());
    match ix_type {
        bubblegum::InstructionName::Transfer => {
            println!("BGUM: Transfer");
            let gummy_roll_events = get_gummy_roll_events(logs)?;
            let leaf_event = get_leaf_event(logs)?;
            db.transaction::<_, _, IngesterError>(|txn| {
                Box::pin(async move {
                    let seq = save_changelog_events(gummy_roll_events, slot, txn)
                        .await?
                        .iter()
                        .max()
                        .map(|v| *v)
                        .ok_or(IngesterError::ChangeLogEventMalformed)?;
                    match leaf_event.schema {
                        LeafSchema::V1 {
                            id,
                            delegate,
                            owner,
                            ..
                        } => {
                            let id_bytes = id.to_bytes().to_vec();
                            let delegate = if owner == delegate {
                                None
                            } else {
                                Some(delegate.to_bytes().to_vec())
                            };
                            let owner_bytes = owner.to_bytes().to_vec();
                            let asset_to_update = asset::ActiveModel {
                                id: Unchanged(id_bytes.clone()),
                                leaf: Set(Some(leaf_event.schema.to_node().to_vec())),
                                delegate: Set(delegate),
                                owner: Set(owner_bytes),
                                seq: Set(seq as i64), // gummyroll seq
                                ..Default::default()
                            };
                            update_asset(txn, id_bytes, Some(seq), asset_to_update).await
                        }
                        _ => Err(IngesterError::NotImplemented),
                    }
                })
            })
            .await?;
        }
        bubblegum::InstructionName::Burn => {
            println!("BGUM: Burn");
            let gummy_roll_events = get_gummy_roll_events(logs)?;
            let leaf_event = get_leaf_event(logs)?;
            db.transaction::<_, _, IngesterError>(|txn| {
                Box::pin(async move {
                    let _seq = save_changelog_events(gummy_roll_events, slot, txn)
                        .await?
                        .iter()
                        .max()
                        .map(|v| *v)
                        .ok_or(IngesterError::ChangeLogEventMalformed)?;
                    match leaf_event.schema {
                        LeafSchema::V1 { id, .. } => {
                            let id_bytes = id.to_bytes().to_vec();
                            let asset_to_update = asset::ActiveModel {
                                id: Unchanged(id_bytes.clone()),
                                burnt: Set(true),
                                ..Default::default()
                            };
                            // Don't send sequence number with this update, because we will always
                            // run this update even if it's from a backfill/replay.
                            update_asset(txn, id_bytes, None, asset_to_update).await
                        }
                        _ => Err(IngesterError::NotImplemented),
                    }
                })
            })
            .await?;
        }
        bubblegum::InstructionName::Delegate => {
            println!("BGUM: Delegate");
            let gummy_roll_events = get_gummy_roll_events(logs)?;
            let leaf_event = get_leaf_event(logs)?;
            db.transaction::<_, _, IngesterError>(|txn| {
                Box::pin(async move {
                    let seq = save_changelog_events(gummy_roll_events, slot, txn)
                        .await?
                        .iter()
                        .max()
                        .map(|v| *v)
                        .ok_or(IngesterError::ChangeLogEventMalformed)?;
                    match leaf_event.schema {
                        LeafSchema::V1 {
                            id,
                            delegate,
                            owner,
                            ..
                        } => {
                            let id_bytes = id.to_bytes().to_vec();
                            let delegate = if owner == delegate {
                                None
                            } else {
                                Some(delegate.to_bytes().to_vec())
                            };
                            let owner_bytes = owner.to_bytes().to_vec();
                            let asset_to_update = asset::ActiveModel {
                                id: Unchanged(id_bytes.clone()),
                                leaf: Set(Some(leaf_event.schema.to_node().to_vec())),
                                delegate: Set(delegate),
                                owner: Set(owner_bytes),
                                seq: Set(seq as i64), // gummyroll seq
                                ..Default::default()
                            };
                            update_asset(txn, id_bytes, Some(seq), asset_to_update).await
                        }
                        _ => Err(IngesterError::NotImplemented),
                    }
                })
            })
            .await?;
        }
        bubblegum::InstructionName::MintV1 => {
            println!("BGUM: MINT");
            let gummy_roll_events = get_gummy_roll_events(logs)?;
            let leaf_event = get_leaf_event(logs)?;
            let data = instruction.data().unwrap()[8..].to_owned();
            let data_buf = &mut data.as_slice();
            let ix: bubblegum::instruction::MintV1 =
                bubblegum::instruction::MintV1::deserialize(data_buf).unwrap();
            let accounts = instruction.accounts().unwrap();
            let update_authority = bytes_from_fb_table(keys, accounts[0] as usize);
            let owner = bytes_from_fb_table(keys, accounts[4] as usize);
            let delegate = bytes_from_fb_table(keys, accounts[5] as usize);
            let merkle_slab = bytes_from_fb_table(keys, accounts[6] as usize);
            let metadata = ix.message.clone();
            let asset_data_id = db
                .transaction::<_, i64, IngesterError>(|txn| {
                    Box::pin(async move {
                        let seq = save_changelog_events(gummy_roll_events, slot, txn)
                            .await?
                            .iter()
                            .max()
                            .map(|v| *v)
                            .ok_or(IngesterError::ChangeLogEventMalformed)?;
                        match leaf_event.schema {
                            LeafSchema::V1 { nonce, id, .. } => {
                                // Printing metadata instruction arguments for debugging
                                println!(
                                    "\tMetadata info: {} {} {} {} {}",
                                    id.to_string(),
                                    &metadata.name,
                                    metadata.seller_fee_basis_points,
                                    metadata.primary_sale_happened,
                                    metadata.is_mutable,
                                );

                                // Insert into `asset_data` table.  Note that if a transaction is
                                // replayed, this will insert the data again resulting in a
                                // duplicate entry.
                                let chain_data = ChainDataV1 {
                                    name: metadata.name,
                                    symbol: metadata.symbol,
                                    edition_nonce: metadata.edition_nonce,
                                    primary_sale_happened: metadata.primary_sale_happened,
                                    token_standard: metadata
                                        .token_standard
                                        .and_then(|ts| TokenStandard::from_u8(ts as u8)),
                                    uses: metadata.uses.map(|u| Uses {
                                        use_method: UseMethod::from_u8(u.use_method as u8).unwrap(),
                                        remaining: u.remaining,
                                        total: u.total,
                                    }),
                                };
                                let chain_data_json =
                                    serde_json::to_value(chain_data).map_err(|e| {
                                        IngesterError::DeserializationError(e.to_string())
                                    })?;
                                let chain_mutability = match metadata.is_mutable {
                                    true => ChainMutability::Mutable,
                                    false => ChainMutability::Immutable,
                                };

                                let data = asset_data::ActiveModel {
                                    chain_data_mutability: Set(chain_mutability),
                                    schema_version: Set(1),
                                    chain_data: Set(chain_data_json),
                                    metadata_url: Set(metadata.uri),
                                    metadata: Set(JsonValue::String("processing".to_string())),
                                    metadata_mutability: Set(Mutability::Mutable),
                                    ..Default::default()
                                }
                                .insert(txn)
                                .await?;

                                // Insert into `asset` table.
                                let delegate = if owner == delegate {
                                    None
                                } else {
                                    Some(delegate)
                                };
                                let model = asset::ActiveModel {
                                    id: Set(id.to_bytes().to_vec()),
                                    owner: Set(owner),
                                    owner_type: Set(OwnerType::Single),
                                    delegate: Set(delegate),
                                    frozen: Set(false),
                                    supply: Set(1),
                                    supply_mint: Set(None),
                                    compressed: Set(true),
                                    compressible: Set(false),
                                    tree_id: Set(Some(merkle_slab)),
                                    specification_version: Set(1),
                                    nonce: Set(nonce as i64),
                                    leaf: Set(Some(leaf_event.schema.to_node().to_vec())),
                                    royalty_target_type: Set(RoyaltyTargetType::Creators),
                                    royalty_target: Set(None),
                                    royalty_amount: Set(metadata.seller_fee_basis_points as i32), //basis points
                                    chain_data_id: Set(Some(data.id)),
                                    seq: Set(seq as i64), // gummyroll seq
                                    ..Default::default()
                                };

                                // Do not attempt to modify any existing values:
                                // `ON CONFLICT ('id') DO NOTHING`.
                                let query = asset::Entity::insert(model)
                                    .on_conflict(
                                        OnConflict::columns([asset::Column::Id])
                                            .do_nothing()
                                            .to_owned(),
                                    )
                                    .build(DbBackend::Postgres);
                                txn.execute(query).await?;

                                // Insert into `asset_creators` table.
                                if metadata.creators.len() > 0 {
                                    let mut creators = Vec::with_capacity(metadata.creators.len());
                                    for c in metadata.creators {
                                        creators.push(asset_creators::ActiveModel {
                                            asset_id: Set(id.to_bytes().to_vec()),
                                            creator: Set(c.address.to_bytes().to_vec()),
                                            share: Set(c.share as i32),
                                            verified: Set(c.verified),
                                            seq: Set(seq as i64), // gummyroll seq
                                            ..Default::default()
                                        });
                                    }

                                    // Do not attempt to modify any existing values:
                                    // `ON CONFLICT ('asset_id') DO NOTHING`.
                                    let query = asset_creators::Entity::insert_many(creators)
                                        .on_conflict(
                                            OnConflict::columns([asset_creators::Column::AssetId])
                                                .do_nothing()
                                                .to_owned(),
                                        )
                                        .build(DbBackend::Postgres);
                                    txn.execute(query).await?;

                                    // Insert into `asset_authority` table.
                                    let model = asset_authority::ActiveModel {
                                        asset_id: Set(id.to_bytes().to_vec()),
                                        authority: Set(update_authority),
                                        seq: Set(seq as i64), // gummyroll seq
                                        ..Default::default()
                                    };

                                    // Do not attempt to modify any existing values:
                                    // `ON CONFLICT ('asset_id') DO NOTHING`.
                                    let query = asset_authority::Entity::insert(model)
                                        .on_conflict(
                                            OnConflict::columns([asset_authority::Column::AssetId])
                                                .do_nothing()
                                                .to_owned(),
                                        )
                                        .build(DbBackend::Postgres);
                                    txn.execute(query).await?;

                                    // Insert into `asset_grouping` table.
                                    if let Some(c) = metadata.collection {
                                        if c.verified {
                                            let model = asset_grouping::ActiveModel {
                                                asset_id: Set(id.to_bytes().to_vec()),
                                                group_key: Set("collection".to_string()),
                                                group_value: Set(c.key.to_string()),
                                                seq: Set(seq as i64), // gummyroll seq
                                                ..Default::default()
                                            };

                                            // Do not attempt to modify any existing values:
                                            // `ON CONFLICT ('asset_id') DO NOTHING`.
                                            let query = asset_grouping::Entity::insert(model)
                                                .on_conflict(
                                                    OnConflict::columns([
                                                        asset_grouping::Column::AssetId,
                                                    ])
                                                    .do_nothing()
                                                    .to_owned(),
                                                )
                                                .build(DbBackend::Postgres);
                                            txn.execute(query).await?;
                                        }
                                    }
                                }
                                Ok(data.id)
                            }
                            _ => Err(IngesterError::NotImplemented),
                        }
                    })
                })
                .await?;
            let task = Some(DownloadMetadata {
                asset_data_id,
                uri: ix.message.uri.clone(),
            });
            task_manager.send(Box::new(task.unwrap()))?;
        }
        bubblegum::InstructionName::Redeem => {
            println!("BGUM: Redeem");
            let gummy_roll_events = get_gummy_roll_events(logs)?;
            let leaf_event = get_leaf_event(logs)?;
            db.transaction::<_, _, IngesterError>(|txn| {
                Box::pin(async move {
                    let seq = save_changelog_events(gummy_roll_events, slot, txn)
                        .await?
                        .iter()
                        .max()
                        .map(|v| *v)
                        .ok_or(IngesterError::ChangeLogEventMalformed)?;
                    match leaf_event.schema {
                        LeafSchema::V1 {
                            id,
                            delegate,
                            owner,
                            ..
                        } => {
                            let id_bytes = id.to_bytes().to_vec();
                            let delegate = if owner == delegate {
                                None
                            } else {
                                Some(delegate.to_bytes().to_vec())
                            };
                            let owner_bytes = owner.to_bytes().to_vec();
                            let asset_to_update = asset::ActiveModel {
                                id: Unchanged(id_bytes.clone()),
                                leaf: Set(Some(vec![0; 32])),
                                delegate: Set(delegate),
                                owner: Set(owner_bytes),
                                seq: Set(seq as i64), // gummyroll seq
                                ..Default::default()
                            };
                            update_asset(txn, id_bytes, Some(seq), asset_to_update).await
                        }
                        _ => Err(IngesterError::NotImplemented),
                    }
                })
            })
            .await?;
        }
        bubblegum::InstructionName::CancelRedeem => {
            println!("BGUM: Cancel Redeem");
            let gummy_roll_events = get_gummy_roll_events(logs)?;
            let leaf_event = get_leaf_event(logs)?;
            db.transaction::<_, _, IngesterError>(|txn| {
                Box::pin(async move {
                    let seq = save_changelog_events(gummy_roll_events, slot, txn)
                        .await?
                        .iter()
                        .max()
                        .map(|v| *v)
                        .ok_or(IngesterError::ChangeLogEventMalformed)?;
                    match leaf_event.schema {
                        LeafSchema::V1 {
                            id,
                            delegate,
                            owner,
                            ..
                        } => {
                            let id_bytes = id.to_bytes().to_vec();
                            let delegate = if owner == delegate {
                                None
                            } else {
                                Some(delegate.to_bytes().to_vec())
                            };
                            let owner_bytes = owner.to_bytes().to_vec();
                            let asset_to_update = asset::ActiveModel {
                                id: Unchanged(id_bytes.clone()),
                                leaf: Set(Some(leaf_event.schema.to_node().to_vec())),
                                delegate: Set(delegate),
                                owner: Set(owner_bytes),
                                seq: Set(seq as i64), // gummyroll seq
                                ..Default::default()
                            };
                            update_asset(txn, id_bytes, Some(seq), asset_to_update).await
                        }
                        _ => Err(IngesterError::NotImplemented),
                    }
                })
            })
            .await?;
        }
        bubblegum::InstructionName::DecompressV1 => {
            println!("BGUM: Decompress");
            let decompress_event = get_decompress_event(logs)?;
            db.transaction::<_, _, IngesterError>(|txn| {
                Box::pin(async move {
                    match decompress_event.version {
                        Version::V1 => {
                            let id_bytes = decompress_event.id.to_bytes().to_vec();
                            let model = asset::ActiveModel {
                                id: Unchanged(id_bytes.clone()),
                                leaf: Set(None),
                                compressed: Set(false),
                                compressible: Set(false),
                                supply: Set(1),
                                supply_mint: Set(Some(id_bytes.clone())),
                                ..Default::default()
                            };

                            // After the decompress instruction runs, the asset is no longer managed
                            // by Bubblegum and Gummyroll, so there will not be any other instructions
                            // after this one.
                            //
                            // Do not run this command if the asset is already marked as
                            // decompressed.
                            let query = asset::Entity::update(model)
                                .filter(asset::Column::Id.eq(id_bytes))
                                .filter(asset::Column::Compressed.eq(true))
                                .build(DbBackend::Postgres);

                            txn.execute(query).await.map(|_| ()).map_err(Into::into)
                        }
                        _ => Err(IngesterError::NotImplemented),
                    }
                })
            })
            .await?;
        }
        _ => println!("Bubblegum: Not Implemented Instruction"),
    }
    Ok(())
}
