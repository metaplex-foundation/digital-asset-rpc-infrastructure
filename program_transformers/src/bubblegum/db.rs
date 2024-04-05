use {
    crate::error::{ProgramTransformerError, ProgramTransformerResult},
    digital_asset_types::dao::{
        asset, asset_authority, asset_creators, asset_data, asset_grouping, cl_audits_v2, cl_items,
        sea_orm_active_enums::{
            ChainMutability, Instruction, Mutability, OwnerType, RoyaltyTargetType,
            SpecificationAssetClass, SpecificationVersions,
        },
    },
    mpl_bubblegum::types::{Collection, Creator},
    sea_orm::{
        entity::{ActiveValue, EntityTrait},
        query::{JsonValue, QueryTrait},
        sea_query::query::OnConflict,
        ConnectionTrait, DbBackend, TransactionTrait,
    },
    spl_account_compression::events::ChangeLogEventV1,
    tracing::{debug, error},
};

pub async fn save_changelog_event<'c, T>(
    change_log_event: &ChangeLogEventV1,
    slot: u64,
    txn_id: &str,
    txn: &T,
    instruction: &str,
    cl_audits: bool,
) -> ProgramTransformerResult<u64>
where
    T: ConnectionTrait + TransactionTrait,
{
    insert_change_log(change_log_event, slot, txn_id, txn, instruction, cl_audits).await?;
    Ok(change_log_event.seq)
}

const fn node_idx_to_leaf_idx(index: i64, tree_height: u32) -> i64 {
    index - 2i64.pow(tree_height)
}

pub async fn insert_change_log<'c, T>(
    change_log_event: &ChangeLogEventV1,
    _slot: u64,
    txn_id: &str,
    txn: &T,
    instruction: &str,
    cl_audits: bool,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let depth = change_log_event.path.len() - 1;
    let tree_id = change_log_event.id.as_ref();
    for (i, p) in (0_i64..).zip(change_log_event.path.iter()) {
        let node_idx = p.index as i64;
        debug!(
            "seq {}, index {} level {}, node {:?}, txn: {:?}, instruction {}",
            change_log_event.seq,
            p.index,
            i,
            bs58::encode(p.node).into_string(),
            txn_id,
            instruction
        );
        let leaf_idx = if i == 0 {
            Some(node_idx_to_leaf_idx(node_idx, depth as u32))
        } else {
            None
        };

        let item = cl_items::ActiveModel {
            tree: ActiveValue::Set(tree_id.to_vec()),
            level: ActiveValue::Set(i),
            node_idx: ActiveValue::Set(node_idx),
            hash: ActiveValue::Set(p.node.as_ref().to_vec()),
            seq: ActiveValue::Set(change_log_event.seq as i64),
            leaf_idx: ActiveValue::Set(leaf_idx),
            ..Default::default()
        };

        let mut query = cl_items::Entity::insert(item)
            .on_conflict(
                OnConflict::columns([cl_items::Column::Tree, cl_items::Column::NodeIdx])
                    .update_columns([
                        cl_items::Column::Hash,
                        cl_items::Column::Seq,
                        cl_items::Column::LeafIdx,
                        cl_items::Column::Level,
                    ])
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        query.sql = format!("{} WHERE excluded.seq >= cl_items.seq", query.sql);
        txn.execute(query)
            .await
            .map_err(|db_err| ProgramTransformerError::StorageWriteError(db_err.to_string()))?;
    }

    // Insert the audit item after the insert into cl_items have been completed
    if cl_audits {
        let tx_id_bytes = bs58::decode(txn_id)
            .into_vec()
            .map_err(|_e| ProgramTransformerError::ChangeLogEventMalformed)?;
        let ix = Instruction::from(instruction);
        if ix == Instruction::Unknown {
            error!("Unknown instruction: {}", instruction);
        }
        let audit_item_v2 = cl_audits_v2::ActiveModel {
            tree: ActiveValue::Set(tree_id.to_vec()),
            leaf_idx: ActiveValue::Set(change_log_event.index as i64),
            seq: ActiveValue::Set(change_log_event.seq as i64),
            tx: ActiveValue::Set(tx_id_bytes),
            instruction: ActiveValue::Set(ix),
            ..Default::default()
        };
        let query = cl_audits_v2::Entity::insert(audit_item_v2)
            .on_conflict(
                OnConflict::columns([
                    cl_audits_v2::Column::Tree,
                    cl_audits_v2::Column::LeafIdx,
                    cl_audits_v2::Column::Seq,
                ])
                .do_nothing()
                .to_owned(),
            )
            .build(DbBackend::Postgres);
        match txn.execute(query).await {
            Ok(_) => {}
            Err(e) => {
                error!("Error while inserting into cl_audits_v2: {:?}", e);
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_asset_with_leaf_info<T>(
    txn: &T,
    id: Vec<u8>,
    nonce: i64,
    tree_id: Vec<u8>,
    leaf: Vec<u8>,
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    seq: i64,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let data_hash = bs58::encode(data_hash).into_string().trim().to_string();
    let creator_hash = bs58::encode(creator_hash).into_string().trim().to_string();
    let model = asset::ActiveModel {
        id: ActiveValue::Set(id),
        nonce: ActiveValue::Set(Some(nonce)),
        tree_id: ActiveValue::Set(Some(tree_id)),
        leaf: ActiveValue::Set(Some(leaf)),
        data_hash: ActiveValue::Set(Some(data_hash)),
        creator_hash: ActiveValue::Set(Some(creator_hash)),
        leaf_seq: ActiveValue::Set(Some(seq)),
        ..Default::default()
    };

    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset::Column::Id)
                .update_columns([
                    asset::Column::Nonce,
                    asset::Column::TreeId,
                    asset::Column::Leaf,
                    asset::Column::DataHash,
                    asset::Column::CreatorHash,
                    asset::Column::LeafSeq,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    // Do not overwrite changes that happened after decompression (asset.seq = 0).
    // Do not overwrite changes from a later Bubblegum instruction.
    query.sql = format!(
        "{} WHERE (asset.seq != 0 OR asset.seq IS NULL) AND (excluded.leaf_seq >= asset.leaf_seq OR asset.leaf_seq IS NULL)",
        query.sql
    );

    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_asset_with_owner_and_delegate_info<T>(
    txn: &T,
    id: Vec<u8>,
    owner: Vec<u8>,
    delegate: Option<Vec<u8>>,
    seq: i64,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset::ActiveModel {
        id: ActiveValue::Set(id),
        owner: ActiveValue::Set(Some(owner)),
        delegate: ActiveValue::Set(delegate),
        owner_delegate_seq: ActiveValue::Set(Some(seq)),
        ..Default::default()
    };

    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset::Column::Id)
                .update_columns([
                    asset::Column::Owner,
                    asset::Column::Delegate,
                    asset::Column::OwnerDelegateSeq,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    // Do not overwrite changes that happened after decompression (asset.seq = 0).
    // Do not overwrite changes from a later Bubblegum instruction.
    query.sql = format!(
            "{} WHERE (asset.seq != 0 OR asset.seq IS NULL) AND (excluded.owner_delegate_seq >= asset.owner_delegate_seq OR asset.owner_delegate_seq IS NULL)",
            query.sql
        );

    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_asset_with_compression_info<T>(
    txn: &T,
    id: Vec<u8>,
    compressed: bool,
    compressible: bool,
    supply: i64,
    supply_mint: Option<Vec<u8>>,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset::ActiveModel {
        id: ActiveValue::Set(id),
        compressed: ActiveValue::Set(compressed),
        compressible: ActiveValue::Set(compressible),
        supply: ActiveValue::Set(supply),
        supply_mint: ActiveValue::Set(supply_mint),
        ..Default::default()
    };

    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::Compressed,
                    asset::Column::Compressible,
                    asset::Column::Supply,
                    asset::Column::SupplyMint,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    // Do not overwrite changes that happened after decompression (asset.seq = 0).
    query.sql = format!("{} WHERE asset.seq != 0 OR asset.seq IS NULL", query.sql);
    txn.execute(query).await?;

    Ok(())
}

pub async fn upsert_asset_with_seq<T>(
    txn: &T,
    id: Vec<u8>,
    seq: i64,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset::ActiveModel {
        id: ActiveValue::Set(id),
        seq: ActiveValue::Set(Some(seq)),
        ..Default::default()
    };

    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset::Column::Id)
                .update_columns([asset::Column::Seq])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    // Do not overwrite changes that happened after decompression (asset.seq = 0).
    // Do not overwrite changes from a later Bubblegum instruction.
    query.sql = format!(
        "{} WHERE (asset.seq != 0 AND excluded.seq >= asset.seq) OR asset.seq IS NULL",
        query.sql
    );

    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_collection_info<T>(
    txn: &T,
    asset_id: Vec<u8>,
    collection: Option<Collection>,
    slot_updated: i64,
    seq: i64,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let (group_value, verified) = match collection {
        Some(c) => (Some(c.key.to_string()), c.verified),
        None => (None, false),
    };

    let model = asset_grouping::ActiveModel {
        asset_id: ActiveValue::Set(asset_id),
        group_key: ActiveValue::Set("collection".to_string()),
        group_value: ActiveValue::Set(group_value),
        verified: ActiveValue::Set(verified),
        slot_updated: ActiveValue::Set(Some(slot_updated)),
        group_info_seq: ActiveValue::Set(Some(seq)),
        ..Default::default()
    };

    let mut query = asset_grouping::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([
                asset_grouping::Column::AssetId,
                asset_grouping::Column::GroupKey,
            ])
            .update_columns([
                asset_grouping::Column::GroupValue,
                asset_grouping::Column::Verified,
                asset_grouping::Column::SlotUpdated,
                asset_grouping::Column::GroupInfoSeq,
            ])
            .to_owned(),
        )
        .build(DbBackend::Postgres);

    // Do not overwrite changes that happened after decompression (asset_grouping.group_info_seq = 0).
    query.sql = format!(
        "{} WHERE (asset_grouping.group_info_seq != 0 AND excluded.group_info_seq >= asset_grouping.group_info_seq) OR asset_grouping.group_info_seq IS NULL",
        query.sql
    );

    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_asset_data<T>(
    txn: &T,
    id: Vec<u8>,
    chain_data_mutability: ChainMutability,
    chain_data: JsonValue,
    metadata_url: String,
    metadata_mutability: Mutability,
    metadata: JsonValue,
    slot_updated: i64,
    reindex: Option<bool>,
    raw_name: Vec<u8>,
    raw_symbol: Vec<u8>,
    seq: i64,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset_data::ActiveModel {
        id: ActiveValue::Set(id.clone()),
        chain_data_mutability: ActiveValue::Set(chain_data_mutability),
        chain_data: ActiveValue::Set(chain_data),
        metadata_url: ActiveValue::Set(metadata_url),
        metadata_mutability: ActiveValue::Set(metadata_mutability),
        metadata: ActiveValue::Set(metadata),
        slot_updated: ActiveValue::Set(slot_updated),
        reindex: ActiveValue::Set(reindex),
        raw_name: ActiveValue::Set(Some(raw_name)),
        raw_symbol: ActiveValue::Set(Some(raw_symbol)),
        base_info_seq: ActiveValue::Set(Some(seq)),
    };

    let mut query = asset_data::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset_data::Column::Id])
                .update_columns([
                    asset_data::Column::ChainDataMutability,
                    asset_data::Column::ChainData,
                    asset_data::Column::MetadataUrl,
                    asset_data::Column::MetadataMutability,
                    // Don't update asset_data::Column::Metadata if it already exists.  Even if we
                    // are indexing `update_metadata`` and there's a new URI, the new background
                    // task will overwrite it.
                    asset_data::Column::SlotUpdated,
                    asset_data::Column::Reindex,
                    asset_data::Column::RawName,
                    asset_data::Column::RawSymbol,
                    asset_data::Column::BaseInfoSeq,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    // Do not overwrite changes that happened after decompression (asset_data.base_info_seq = 0).
    // Do not overwrite changes from a later Bubblegum instruction.
    query.sql = format!(
        "{} WHERE (asset_data.base_info_seq != 0 AND excluded.base_info_seq >= asset_data.base_info_seq) OR asset_data.base_info_seq IS NULL",
        query.sql
    );
    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_asset_base_info<T>(
    txn: &T,
    id: Vec<u8>,
    owner_type: OwnerType,
    frozen: bool,
    specification_version: SpecificationVersions,
    specification_asset_class: SpecificationAssetClass,
    royalty_target_type: RoyaltyTargetType,
    royalty_target: Option<Vec<u8>>,
    royalty_amount: i32,
    slot_updated: i64,
    seq: i64,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    // Set base info for asset.
    let asset_model = asset::ActiveModel {
        id: ActiveValue::Set(id.clone()),
        owner_type: ActiveValue::Set(owner_type),
        frozen: ActiveValue::Set(frozen),
        specification_version: ActiveValue::Set(Some(specification_version)),
        specification_asset_class: ActiveValue::Set(Some(specification_asset_class)),
        royalty_target_type: ActiveValue::Set(royalty_target_type),
        royalty_target: ActiveValue::Set(royalty_target),
        royalty_amount: ActiveValue::Set(royalty_amount),
        asset_data: ActiveValue::Set(Some(id.clone())),
        slot_updated_cnft_transaction: ActiveValue::Set(Some(slot_updated)),
        base_info_seq: ActiveValue::Set(Some(seq)),
        ..Default::default()
    };

    // Upsert asset table base info.
    let mut query = asset::Entity::insert(asset_model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::OwnerType,
                    asset::Column::Frozen,
                    asset::Column::SpecificationVersion,
                    asset::Column::SpecificationAssetClass,
                    asset::Column::RoyaltyTargetType,
                    asset::Column::RoyaltyTarget,
                    asset::Column::RoyaltyAmount,
                    asset::Column::AssetData,
                    asset::Column::SlotUpdatedCnftTransaction,
                    asset::Column::BaseInfoSeq,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    query.sql = format!(
            "{} WHERE (asset.seq != 0 OR asset.seq IS NULL) AND (excluded.base_info_seq >= asset.base_info_seq OR asset.base_info_seq IS NULL)",
            query.sql
        );

    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_asset_creators<T>(
    txn: &T,
    id: Vec<u8>,
    creators: &Vec<Creator>,
    slot_updated: i64,
    seq: i64,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let db_creators = if creators.is_empty() {
        // If creators are empty, insert an empty creator with the current sequence.
        // This prevents accidental errors during out-of-order updates.
        vec![asset_creators::ActiveModel {
            asset_id: ActiveValue::Set(id.clone()),
            position: ActiveValue::Set(0),
            creator: ActiveValue::Set(vec![]),
            share: ActiveValue::Set(100),
            verified: ActiveValue::Set(false),
            slot_updated: ActiveValue::Set(Some(slot_updated)),
            seq: ActiveValue::Set(Some(seq)),
            ..Default::default()
        }]
    } else {
        creators
            .iter()
            .enumerate()
            .map(|(i, c)| asset_creators::ActiveModel {
                asset_id: ActiveValue::Set(id.clone()),
                position: ActiveValue::Set(i as i16),
                creator: ActiveValue::Set(c.address.to_bytes().to_vec()),
                share: ActiveValue::Set(c.share as i32),
                verified: ActiveValue::Set(c.verified),
                slot_updated: ActiveValue::Set(Some(slot_updated)),
                seq: ActiveValue::Set(Some(seq)),
                ..Default::default()
            })
            .collect()
    };

    // This statement will update base information for each creator.
    let mut query = asset_creators::Entity::insert_many(db_creators)
        .on_conflict(
            OnConflict::columns([
                asset_creators::Column::AssetId,
                asset_creators::Column::Position,
            ])
            .update_columns([
                asset_creators::Column::Creator,
                asset_creators::Column::Share,
                asset_creators::Column::Verified,
                asset_creators::Column::Seq,
                asset_creators::Column::SlotUpdated,
            ])
            .to_owned(),
        )
        .build(DbBackend::Postgres);

    query.sql = format!(
        "{} WHERE (asset_creators.seq != 0 AND excluded.seq >= asset_creators.seq) OR asset_creators.seq IS NULL",
        query.sql
    );

    txn.execute(query).await?;

    Ok(())
}

pub async fn upsert_asset_authority<T>(
    txn: &T,
    asset_id: Vec<u8>,
    authority: Vec<u8>,
    slot_updated: i64,
    seq: i64,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset_authority::ActiveModel {
        asset_id: ActiveValue::Set(asset_id),
        authority: ActiveValue::Set(authority),
        seq: ActiveValue::Set(seq),
        slot_updated: ActiveValue::Set(slot_updated),
        ..Default::default()
    };

    // This value is only written during `mint_V1`` or after an item is decompressed, so do not
    // attempt to modify any existing values:
    // `ON CONFLICT ('asset_id') DO NOTHING`.
    let query = asset_authority::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset_authority::Column::AssetId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    Ok(())
}
