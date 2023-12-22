use crate::error::IngesterError;
use digital_asset_types::dao::{
    asset, asset_creators, asset_grouping, cl_audits_v2, cl_items,
    sea_orm_active_enums::BubblegumInstruction,
};
use log::{debug, error, info};
use mpl_bubblegum::types::Collection;
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbBackend,
    EntityTrait,
};
use solana_sdk::signature::Signature;
use spl_account_compression::events::ChangeLogEventV1;

use std::convert::From;
use std::str::FromStr;

pub async fn save_changelog_event<'c, T>(
    change_log_event: &ChangeLogEventV1,
    txn_id: &str,
    txn: &T,
    instruction: &str,
) -> Result<u64, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    insert_change_log(change_log_event, txn_id, txn, instruction).await?;
    Ok(change_log_event.seq)
}

fn node_idx_to_leaf_idx(index: i64, tree_height: u32) -> i64 {
    index - 2i64.pow(tree_height)
}

pub async fn insert_change_log<'c, T>(
    change_log_event: &ChangeLogEventV1,
    txn_id: &str,
    txn: &T,
    instruction: &str,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let depth = change_log_event.path.len() - 1;
    let tree_id = change_log_event.id.as_ref();
    let signature = Signature::from_str(txn_id)?;
    let leaf_idx = node_idx_to_leaf_idx(
        i64::from(
            change_log_event
                .path
                .get(0)
                .ok_or(IngesterError::MissingChangeLogPath)?
                .index,
        ),
        u32::try_from(depth)?,
    );

    for (i, p) in change_log_event.path.iter().enumerate() {
        let node_idx = p.index as i64;
        info!(
            "seq {}, index {} level {}, node {}, txn {}, instruction {}",
            change_log_event.seq,
            p.index,
            i,
            bs58::encode(p.node).into_string(),
            txn_id,
            instruction
        );
        let leaf_idx = if i == 0 { Some(leaf_idx) } else { None };

        let item = cl_items::ActiveModel {
            tree: Set(tree_id.to_vec()),
            level: Set(i64::try_from(i)?),
            node_idx: Set(node_idx),
            hash: Set(p.node.as_ref().to_vec()),
            seq: Set(change_log_event.seq as i64),
            leaf_idx: Set(leaf_idx),
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
        query.sql = format!("{} WHERE excluded.seq > cl_items.seq", query.sql);
        txn.execute(query)
            .await
            .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
    }

    let cl_audit = cl_audits_v2::ActiveModel {
        tree: Set(tree_id.to_vec()),
        leaf_idx: Set(leaf_idx),
        seq: Set(i64::try_from(change_log_event.seq)?),
        tx: Set(signature.as_ref().to_vec()),
        instruction: Set(BubblegumInstruction::from_str(instruction)?),
        ..Default::default()
    };

    let query = cl_audits_v2::Entity::insert(cl_audit)
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

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

    Ok(())
    //TODO -> set maximum size of path and break into multiple statements
}

pub async fn upsert_asset_with_leaf_info<T>(
    txn: &T,
    id: Vec<u8>,
    nonce: i64,
    tree_id: Vec<u8>,
    leaf: Vec<u8>,
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    seq: i64,
    was_decompressed: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let data_hash = bs58::encode(data_hash).into_string().trim().to_string();
    let creator_hash = bs58::encode(creator_hash).into_string().trim().to_string();
    let model = asset::ActiveModel {
        id: Set(id),
        nonce: Set(Some(nonce)),
        tree_id: Set(Some(tree_id)),
        leaf: Set(Some(leaf)),
        data_hash: Set(Some(data_hash)),
        creator_hash: Set(Some(creator_hash)),
        leaf_seq: Set(Some(seq)),
        ..Default::default()
    };

    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset::Column::Id)
                .update_columns([
                    asset::Column::Nonce,
                    asset::Column::TreeId,
                    asset::Column::Leaf,
                    asset::Column::LeafSeq,
                    asset::Column::DataHash,
                    asset::Column::CreatorHash,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    // If we are indexing decompression we will update the leaf regardless of if we have previously
    // indexed decompression and regardless of seq.
    if !was_decompressed {
        query.sql = format!(
            "{} WHERE (NOT asset.was_decompressed) AND (excluded.leaf_seq >= asset.leaf_seq OR asset.leaf_seq IS NULL)",
            query.sql
        );
    }

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_asset_with_leaf_info_for_decompression<T>(
    txn: &T,
    id: Vec<u8>,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset::ActiveModel {
        id: Set(id),
        leaf: Set(None),
        nonce: Set(Some(0)),
        leaf_seq: Set(None),
        data_hash: Set(None),
        creator_hash: Set(None),
        tree_id: Set(None),
        seq: Set(Some(0)),
        ..Default::default()
    };
    let query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset::Column::Id)
                .update_columns([
                    asset::Column::Leaf,
                    asset::Column::LeafSeq,
                    asset::Column::Nonce,
                    asset::Column::DataHash,
                    asset::Column::CreatorHash,
                    asset::Column::TreeId,
                    asset::Column::Seq,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_asset_with_owner_and_delegate_info<T>(
    txn: &T,
    id: Vec<u8>,
    owner: Vec<u8>,
    delegate: Option<Vec<u8>>,
    seq: i64,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset::ActiveModel {
        id: Set(id),
        owner: Set(Some(owner)),
        delegate: Set(delegate),
        owner_delegate_seq: Set(Some(seq)), // gummyroll seq
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
    query.sql = format!(
            "{} WHERE excluded.owner_delegate_seq >= asset.owner_delegate_seq OR asset.owner_delegate_seq IS NULL",
            query.sql
        );

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_asset_with_compression_info<T>(
    txn: &T,
    id: Vec<u8>,
    compressed: bool,
    compressible: bool,
    supply: i64,
    supply_mint: Option<Vec<u8>>,
    was_decompressed: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset::ActiveModel {
        id: Set(id),
        compressed: Set(compressed),
        compressible: Set(compressible),
        supply: Set(supply),
        supply_mint: Set(supply_mint),
        was_decompressed: Set(was_decompressed),
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
                    asset::Column::WasDecompressed,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    query.sql = format!("{} WHERE NOT asset.was_decompressed", query.sql);
    txn.execute(query).await?;

    Ok(())
}

pub async fn upsert_asset_with_seq<T>(txn: &T, id: Vec<u8>, seq: i64) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset::ActiveModel {
        id: Set(id),
        seq: Set(Some(seq)),
        ..Default::default()
    };

    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset::Column::Id)
                .update_columns([asset::Column::Seq])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    query.sql = format!(
        "{} WHERE (NOT asset.was_decompressed) AND (excluded.seq >= asset.seq OR asset.seq IS NULL)",
        query.sql
    );

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_creator_verified<T>(
    txn: &T,
    asset_id: Vec<u8>,
    creator: Vec<u8>,
    verified: bool,
    seq: i64,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset_creators::ActiveModel {
        asset_id: Set(asset_id),
        creator: Set(creator),
        verified: Set(verified),
        seq: Set(Some(seq)),
        ..Default::default()
    };

    let mut query = asset_creators::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([
                asset_creators::Column::AssetId,
                asset_creators::Column::Creator,
            ])
            .update_columns([
                asset_creators::Column::Verified,
                asset_creators::Column::Seq,
            ])
            .to_owned(),
        )
        .build(DbBackend::Postgres);

    query.sql = format!(
        "{} WHERE excluded.seq >= asset_creators.seq OR asset_creators.seq is NULL",
        query.sql
    );

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_collection_info<T>(
    txn: &T,
    asset_id: Vec<u8>,
    collection: Option<Collection>,
    slot_updated: i64,
    seq: i64,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let (group_value, verified) = match collection {
        Some(c) => (Some(c.key.to_string()), c.verified),
        None => (None, false),
    };

    let model = asset_grouping::ActiveModel {
        asset_id: Set(asset_id),
        group_key: Set("collection".to_string()),
        group_value: Set(group_value),
        verified: Set(verified),
        slot_updated: Set(Some(slot_updated)),
        group_info_seq: Set(Some(seq)),
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

    query.sql = format!(
        "{} WHERE excluded.group_info_seq >= asset_grouping.group_info_seq OR asset_grouping.group_info_seq IS NULL",
        query.sql
    );

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}
