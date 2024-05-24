use crate::error::IngesterError;
use anchor_lang::prelude::borsh::{BorshDeserialize, BorshSerialize};
use digital_asset_types::dao::{character_history, compressed_data, merkle_tree};
use hpl_toolkit::{compression::*, schema::*};
use log::{debug, info};
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveValue::Set, ColumnTrait, DbBackend, EntityTrait,
};
use solana_sdk::pubkey::Pubkey;
use spl_account_compression::events::ApplicationDataEventV1;
use std::str::FromStr;

pub async fn save_applicationdata_event<'c, T>(
    application_data: &ApplicationDataEventV1,
    txn: &T,
) -> Result<u64, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    insert_application_data(application_data, txn).await?;
    Ok(0)
}

pub async fn insert_application_data<'c, T>(
    application_data: &ApplicationDataEventV1,
    txn: &T,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    debug!("Inserting AppData");
    let buf = &mut &application_data.application_data[..];
    let event = CompressedDataEvent::deserialize(buf)
        .map_err(|db_err| IngesterError::CompressedDataParseError(db_err.to_string()))?;
    debug!("Application data parsed successfully");
    match event {
        CompressedDataEvent::TreeSchemaValue {
            discriminator,
            tree_id,
            schema,
            program_id,
        } => {
            info!("Found new tree {}", bs58::encode(tree_id).into_string());
            // @TODO: Fetch and store, maxDepth, maxBufferSize, canopyDepth, etc...
            let data_schema = schema
                .try_to_vec()
                .map_err(|db_err| IngesterError::CompressedDataParseError(db_err.to_string()))?;

            debug!("Parsed tree data schema");

            let item = merkle_tree::ActiveModel {
                id: Set(tree_id.to_vec()),
                data_schema: Set(data_schema),
                discriminator: Set(discriminator.to_vec()),
                program: Set(Some(program_id.to_vec())),
                ..Default::default()
            };
            let query = merkle_tree::Entity::insert(item)
                .on_conflict(
                    OnConflict::columns([merkle_tree::Column::Id])
                        .update_columns([merkle_tree::Column::DataSchema])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);
            debug!("Query builed successfully");
            txn.execute(query)
                .await
                .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
            debug!("Query executed successfully");
        }
        CompressedDataEvent::Leaf {
            slot,
            tree_id,
            leaf_idx,
            seq,
            stream_type,
        } => match stream_type {
            CompressedDataEventStream::Full { mut data } => {
                info!(
                    "Found new leaf for {} at index {}",
                    bs58::encode(tree_id).into_string(),
                    leaf_idx
                );
                let tree = merkle_tree::Entity::find_by_id(tree_id.to_vec())
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;

                debug!("Find tree query executed successfully");
                let mut schema_validated: bool = false;
                let mut program_id: Option<Pubkey> = None;
                if let Some(tree) = tree {
                    debug!("Parsing tree data schema");
                    let schema =
                        Schema::deserialize(&mut &tree.data_schema[..]).map_err(|db_err| {
                            IngesterError::CompressedDataParseError(db_err.to_string())
                        })?;
                    if tree.program.is_none() {
                        return Err(IngesterError::CompressedDataParseError(format!(
                            "Tree program not found"
                        )));
                    }
                    program_id = Some(Pubkey::try_from(tree.program.unwrap()).unwrap());
                    debug!("Parsed tree data schema");
                    if !schema.validate(&mut data) {
                        info!("Schema value validation failed");
                        return Err(IngesterError::CompressedDataParseError(format!(
                            "Schema value validation failed for data: {} with schema: {}",
                            data.to_string(),
                            schema.to_string()
                        ))
                        .into());
                    }

                    schema_validated = true;
                }

                debug!("Serializing raw data");
                let raw_data = data.try_to_vec().map_err(|db_err| {
                    IngesterError::CompressedDataParseError(db_err.to_string())
                })?;
                debug!("Serialized raw data");

                let item = compressed_data::ActiveModel {
                    id: Set(anchor_lang::solana_program::keccak::hashv(
                        &[&tree_id[..], &leaf_idx.to_le_bytes()[..]][..],
                    )
                    .to_bytes()
                    .to_vec()),
                    tree_id: Set(tree_id.to_vec()),
                    leaf_idx: Set(leaf_idx as i64),
                    seq: Set(seq as i64),
                    schema_validated: Set(schema_validated),
                    raw_data: Set(raw_data),
                    parsed_data: Set(data.clone().into()),
                    slot_updated: Set(slot as i64),
                    ..Default::default()
                };

                let query = compressed_data::Entity::insert(item)
                    .on_conflict(
                        OnConflict::columns([
                            compressed_data::Column::TreeId,
                            compressed_data::Column::LeafIdx,
                        ])
                        .update_columns([
                            compressed_data::Column::TreeId,
                            compressed_data::Column::LeafIdx,
                            compressed_data::Column::Seq,
                            compressed_data::Column::SchemaValidated,
                            compressed_data::Column::RawData,
                            compressed_data::Column::ParsedData,
                            compressed_data::Column::SlotUpdated,
                        ])
                        .to_owned(),
                    )
                    .build(DbBackend::Postgres);

                debug!("Query builed successfully");
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
                debug!("Query executed successfully");

                let id = anchor_lang::solana_program::keccak::hashv(
                    &[&tree_id[..], &leaf_idx.to_le_bytes()[..]][..],
                )
                .to_bytes()
                .to_vec();
                let found_latest_entry = compressed_data::Entity::find()
                    .filter(compressed_data::Column::Id.eq(id.to_owned()))
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;
                debug!("Find the latest entry data query executed successfully");

                if found_latest_entry.is_none() {
                    return Err(IngesterError::StorageReadError(
                        "Could not find the latest entry data in db".to_string(),
                    ));
                }
                let db_data: compressed_data::ActiveModel = found_latest_entry.unwrap().into();
                debug!("Found new_data {:?}", db_data);

                if let Some(program_id) = program_id {
                    if program_id
                        == Pubkey::from_str("ChRCtrG7X5kb9YncA4wuyD68DXXL8Szt3zBCCGiioBTg").unwrap()
                    {
                        if let SchemaValue::Object(character) = data {
                            if let Some(kind_obj) = character.get(&"used_by".to_string()) {
                                new_character_event(
                                    txn,
                                    db_data.id.unwrap() as Vec<u8>,
                                    kind_obj.clone(),
                                    ("NewCharacter").to_string(),
                                    slot as i64,
                                    // Some(false),
                                )
                                .await?;
                            }
                        }
                    }
                }
            }
            CompressedDataEventStream::PatchChunk { key, data } => {
                info!(
                    "Patch leaf for {} at index {}",
                    bs58::encode(tree_id).into_string(),
                    leaf_idx
                );

                let tree = merkle_tree::Entity::find_by_id(tree_id.to_vec())
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;

                debug!("Find tree query executed successfully");

                let mut program_id: Option<Pubkey> = None;
                if let Some(tree) = tree {
                    program_id = Some(Pubkey::try_from(tree.program.unwrap()).unwrap());
                    debug!("Parsing tree data schema");
                }

                let id = anchor_lang::solana_program::keccak::hashv(
                    &[&tree_id[..], &leaf_idx.to_le_bytes()[..]][..],
                )
                .to_bytes()
                .to_vec();
                let found = compressed_data::Entity::find()
                    .filter(compressed_data::Column::Id.eq(id.to_owned()))
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;

                debug!("Find old_data query executed successfully");

                if found.is_none() {
                    return Err(IngesterError::StorageReadError(
                        "Could not find old data in db".to_string(),
                    ));
                }

                let mut db_data: compressed_data::ActiveModel = found.unwrap().into();
                debug!("Found old_data {:?}", db_data);

                debug!("Wrapped model into ActiveModel");

                let mut parsed_data: JsonValue = db_data.parsed_data.take().unwrap();

                if let JsonValue::Object(object) = &mut parsed_data {
                    if object.contains_key(&key) {
                        debug!("Patching {}: {:?}", key, data.to_string());
                        if key == "used_by".to_string() {
                            if let Some(program_id) = program_id {
                                debug!("program_id {:?}", program_id);
                                if program_id
                                    == Pubkey::from_str(
                                        "ChRCtrG7X5kb9YncA4wuyD68DXXL8Szt3zBCCGiioBTg",
                                    )
                                    .unwrap()
                                {
                                    if let Some(used_by) = object.get("used_by") {
                                        log_character_history(
                                            txn,
                                            id.clone(),
                                            used_by.clone().into(),
                                            data.clone(),
                                            slot as i64,
                                        )
                                        .await?;
                                    }
                                }
                            }
                        }

                        object.insert(key, data.to_owned().into());
                    }
                }

                debug!("Complete Data After Patch: {}", parsed_data.to_string());
                db_data.parsed_data = Set(parsed_data);
                debug!("Data updated in object");

                let query: Statement = compressed_data::Entity::update(db_data.clone())
                    .filter(compressed_data::Column::Id.eq(id))
                    .build(DbBackend::Postgres);

                debug!(
                    "Query builed successfully, {}, values {:#?}",
                    query.sql, query.values
                );
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

                debug!("Query executed successfully");
            }
            CompressedDataEventStream::Empty => {
                info!(
                    "Remove leaf for {} at index {}",
                    bs58::encode(tree_id).into_string(),
                    leaf_idx
                );
                let found = compressed_data::Entity::find()
                    .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
                    .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;

                debug!("Find old_data query executed successfully");

                if found.is_none() {
                    return Err(IngesterError::StorageReadError(
                        "Could not find old data in db".to_string(),
                    ));
                }

                let db_data: compressed_data::ActiveModel = found.unwrap().into();
                debug!("Found old_data {:?}", db_data);

                let query: Statement = compressed_data::Entity::delete(db_data)
                    .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
                    .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
                    .build(DbBackend::Postgres);

                debug!(
                    "Query builed successfully, {}, values {:#?}",
                    query.sql, query.values
                );
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
                debug!("Query executed successfully");
            }
        },
    }
    Ok(())
}

pub async fn log_character_history<T>(
    txn: &T,
    character_id: Vec<u8>,
    pre_used_by: SchemaValue,
    new_used_by: SchemaValue,
    slot: i64,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    debug!("pre_used by {:?}", pre_used_by.to_string());
    debug!("new_used by {:?}", new_used_by.to_string());
    let pre_used_by_kind = match pre_used_by {
        SchemaValue::Enum(kind, _) => kind,
        _ => unreachable!(),
    };

    let new_used_by_kind = match new_used_by.clone() {
        SchemaValue::Enum(kind, _) => kind,
        _ => unreachable!(),
    };

    let event = match (pre_used_by_kind.as_str(), new_used_by_kind.as_str()) {
        ("Ejected", "None") => String::from("Wrapped"),
        ("None", "Staking") => String::from("Staked"),
        ("None", "Mission") => String::from("MissionParticipation"),
        ("Staking", "None") => String::from("UnStaked"),
        ("Staking", "Staking") => String::from("ClaimedStakingReward"),
        ("Mission", "None") => String::from("RecallFromMission"),
        ("Mission", "Mission") => String::from("ClaimedMissionReward"),
        (_, "Ejected") => String::from("UnWrapped"),
        (_, _) => unreachable!(),
    };

    debug!("Event {:?}", event);
    debug!("pre_used_by_kind {:?}", pre_used_by_kind);
    debug!("new_used_by_kind {:?}", new_used_by_kind);
    debug!("Event Matched");

    new_character_event(txn, character_id, new_used_by, event, slot as i64).await
}

pub async fn new_character_event<T>(
    txn: &T,
    character_id: Vec<u8>,
    event_data: SchemaValue,
    event: String,
    slot: i64,
    // fetch_history: Option<bool>,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let new_history = character_history::ActiveModel {
        event: Set(event), //Set(("NewCharacter").to_string()),
        event_data: Set(event_data.into()),
        character_id: Set(character_id),
        slot_updated: Set(slot),
        ..Default::default()
    };
    let query = character_history::Entity::insert(new_history)
        .on_conflict(
            OnConflict::columns([character_history::Column::Id])
                .update_columns([character_history::Column::CharacterId])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    debug!("Query builed successfully for character_history");
    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
    debug!("Query executed successfully for character_history");

    Ok(())
}
