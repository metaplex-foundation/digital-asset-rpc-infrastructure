use crate::error::IngesterError;
use digital_asset_types::dao::{
    asset, asset_creators, asset_grouping, backfill_items, cl_audits, cl_items, tree_transactions,
};
use log::{debug, error, info};
use mpl_bubblegum::types::Collection;
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbBackend,
    EntityTrait,
};
use solana_sdk::pubkey::Pubkey;
use spl_account_compression::events::ChangeLogEventV1;

use std::convert::From;

/// Mark tree transaction as processed. If the transaction already exists, update the `processed_at` field.
///
/// This function takes in a tree ID, slot, transaction ID, and a transaction object.
/// It first checks if a tree transaction with the given transaction ID already exists.
/// If it does, it updates the `processed_at` field of the existing tree transaction with the current time.
/// If it doesn't, it creates a new tree transaction with the provided parameters and saves it.
///
/// # Arguments
///
/// * `tree_id` - A vector of bytes representing the ID of the tree.
/// * `slot` - A 64-bit unsigned integer representing the slot.
/// * `txn_id` - A string slice representing the transaction ID.
/// * `txn` - A reference to a transaction object.
///
/// # Returns
///
/// This function returns a `Result` that contains an empty tuple, or an `IngesterError` if the operation fails.
pub async fn save_tree_transaction<'c, T>(
    tree_id: Vec<u8>,
    slot: u64,
    txn_id: &str,
    txn: &T,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let now = chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(0).ok_or(IngesterError::ChronoFixedOffset)?);

    let tree_transaction = tree_transactions::Entity::find()
        .filter(tree_transactions::Column::Signature.eq(txn_id))
        .one(txn)
        .await?;

    if let Some(tree_transaction) = tree_transaction {
        let mut tree_transaction: tree_transactions::ActiveModel = tree_transaction.into();

        tree_transaction.processed_at = Set(Some(now));

        tree_transaction.save(txn).await?;
    } else {
        let tree = Pubkey::try_from(txn_id)?;

        let tree_transaction = tree_transactions::ActiveModel {
            signature: Set(txn_id.to_string()),
            slot: Set(i64::try_from(slot)?),
            tree: Set(tree.to_string()),
            processed_at: Set(Some(now)),
            ..Default::default()
        };

        tree_transactions::Entity::insert(tree_transaction)
            .on_conflict(
                OnConflict::column(tree_transactions::Column::Signature)
                    .do_nothing()
                    .to_owned(),
            )
            .exec(txn)
            .await?;
    }
    Ok(())
}

pub async fn save_changelog_event<'c, T>(
    change_log_event: &ChangeLogEventV1,
    slot: u64,
    txn_id: &str,
    txn: &T,
    cl_audits: bool,
    instruction: &str,
) -> Result<u64, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    insert_change_log(change_log_event, slot, txn_id, txn, cl_audits, instruction).await?;
    Ok(change_log_event.seq)
}

fn node_idx_to_leaf_idx(index: i64, tree_height: u32) -> i64 {
    index - 2i64.pow(tree_height)
}

pub async fn insert_change_log<'c, T>(
    change_log_event: &ChangeLogEventV1,
    slot: u64,
    txn_id: &str,
    txn: &T,
    cl_audits: bool,
    instruction: &str,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let mut i: i64 = 0;
    let depth = change_log_event.path.len() - 1;
    let tree_id = change_log_event.id.as_ref();
    for p in change_log_event.path.iter() {
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
        let leaf_idx = if i == 0 {
            Some(node_idx_to_leaf_idx(node_idx, depth as u32))
        } else {
            None
        };

        let item = cl_items::ActiveModel {
            tree: Set(tree_id.to_vec()),
            level: Set(i),
            node_idx: Set(node_idx),
            hash: Set(p.node.as_ref().to_vec()),
            seq: Set(change_log_event.seq as i64),
            leaf_idx: Set(leaf_idx),
            ..Default::default()
        };

        let mut audit_item: cl_audits::ActiveModel = item.clone().into();
        audit_item.tx = Set(txn_id.to_string());
        audit_item.instruction = Set(Some(instruction.to_string()));

        i += 1;
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

        // Insert the audit item after the insert into cl_items have been completed
        let query = cl_audits::Entity::insert(audit_item).build(DbBackend::Postgres);
        match txn.execute(query).await {
            Ok(_) => {}
            Err(e) => {
                error!("Error while inserting into cl_audits: {:?}", e);
            }
        }
    }

    // TODO: drop `backfill_items` table if not needed anymore for backfilling

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
