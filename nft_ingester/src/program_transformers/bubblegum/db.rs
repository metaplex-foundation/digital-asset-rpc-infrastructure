use crate::error::IngesterError;
use digital_asset_types::dao::{
    asset, asset_authority, asset_creators, asset_data, asset_grouping, backfill_items, cl_audits,
    cl_items,
    sea_orm_active_enums::{
        ChainMutability, Mutability, OwnerType, RoyaltyTargetType, SpecificationAssetClass,
        SpecificationVersions,
    },
};
use log::{debug, info};
use mpl_bubblegum::types::{Collection, Creator};
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveValue::Set, ColumnTrait, DbBackend, EntityTrait,
};
use spl_account_compression::events::ChangeLogEventV1;
use std::collections::HashSet;

pub async fn save_changelog_event<'c, T>(
    change_log_event: &ChangeLogEventV1,
    slot: u64,
    txn_id: &str,
    txn: &T,
    cl_audits: bool,
) -> Result<u64, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    insert_change_log(change_log_event, slot, txn_id, txn, cl_audits).await?;
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
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let mut i: i64 = 0;
    let depth = change_log_event.path.len() - 1;
    let tree_id = change_log_event.id.as_ref();
    for p in change_log_event.path.iter() {
        let node_idx = p.index as i64;
        debug!(
            "seq {}, index {} level {}, node {:?}, txn: {:?}",
            change_log_event.seq,
            p.index,
            i,
            bs58::encode(p.node).into_string(),
            txn_id,
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

        let audit_item: Option<cl_audits::ActiveModel> = if cl_audits {
            let mut ai: cl_audits::ActiveModel = item.clone().into();
            ai.tx = Set(txn_id.to_string());
            Some(ai)
        } else {
            None
        };

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
        if let Some(audit_item) = audit_item {
            cl_audits::Entity::insert(audit_item).exec(txn).await?;
        }
    }

    // If and only if the entire path of nodes was inserted into the `cl_items` table, then insert
    // a single row into the `backfill_items` table.  This way if an incomplete path was inserted
    // into `cl_items` due to an error, a gap will be created for the tree and the backfiller will
    // fix it.
    if i - 1 == depth as i64 {
        // See if the tree already exists in the `backfill_items` table.
        let rows = backfill_items::Entity::find()
            .filter(backfill_items::Column::Tree.eq(tree_id))
            .limit(1)
            .all(txn)
            .await?;

        // If the tree does not exist in `backfill_items` and the sequence number is greater than 1,
        // then we know we will need to backfill the tree from sequence number 1 up to the current
        // sequence number.  So in this case we set at flag to force checking the tree.
        let force_chk = rows.is_empty() && change_log_event.seq > 1;

        info!("Adding to backfill_items table at level {}", i - 1);
        let item = backfill_items::ActiveModel {
            tree: Set(tree_id.to_vec()),
            seq: Set(change_log_event.seq as i64),
            slot: Set(slot as i64),
            force_chk: Set(force_chk),
            backfilled: Set(false),
            failed: Set(false),
            ..Default::default()
        };

        backfill_items::Entity::insert(item).exec(txn).await?;
    }

    Ok(())
    //TODO -> set maximum size of path and break into multiple statements
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
                    asset::Column::DataHash,
                    asset::Column::CreatorHash,
                    asset::Column::LeafSeq,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    // Do not overwrite changes that happened after decompression (asset.seq = 0).
    // If the asset was decompressed, don't update the leaf info since we cleared it during decompression.
    query.sql = format!(
        "{} WHERE asset.seq != 0 AND (NOT asset.was_decompressed) AND (excluded.leaf_seq >= asset.leaf_seq OR asset.leaf_seq IS NULL)",
        query.sql
    );

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
        nonce: Set(Some(0)),
        tree_id: Set(None),
        leaf: Set(None),
        data_hash: Set(None),
        creator_hash: Set(None),
        leaf_seq: Set(None),
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
    query.sql = format!("{} WHERE asset.seq != 0", query.sql);

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

    // Do not overwrite changes that happened after decompression (asset.seq = 0).
    // Do not overwrite changes from a later Bubblegum instruction.
    query.sql = format!(
            "{} WHERE asset.seq != 0 AND (excluded.owner_delegate_seq >= asset.owner_delegate_seq OR asset.owner_delegate_seq IS NULL)",
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

    // Do not overwrite changes that happened after decompression (asset.seq = 0).
    // Do not overwrite changes from Bubblegum decompress instruction itself.
    query.sql = format!(
        "{} WHERE asset.seq != 0 AND (NOT asset.was_decompressed)",
        query.sql
    );
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

    // Do not overwrite changes that happened after decompression (asset.seq = 0).
    // Do not overwrite changes from a later Bubblegum instruction.
    query.sql = format!(
        "{} WHERE (asset.seq != 0 AND excluded.seq >= asset.seq) OR asset.seq IS NULL",
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
        asset_id: Set(asset_id.clone()),
        creator: Set(creator),
        verified: Set(verified),
        verified_seq: Set(Some(seq)),
        ..Default::default()
    };

    // Only upsert a creator if the asset table's creator array seq is at a lower value.  That seq
    // gets updated when we set up the creator array in `mintV1` or `update_metadata`.  We don't
    // want to insert a creator that was removed from a later `update_metadata`.  And we don't need
    // to worry about creator verification in that case because the `update_metadata` updates
    // creator verification state as well.
    let multi_txn = txn.begin().await?;
    if creators_should_be_updated(&multi_txn, asset_id, seq).await? {
        let mut query = asset_creators::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    asset_creators::Column::AssetId,
                    asset_creators::Column::Creator,
                ])
                .update_columns([
                    asset_creators::Column::Verified,
                    asset_creators::Column::VerifiedSeq,
                ])
                .to_owned(),
            )
            .build(DbBackend::Postgres);

        query.sql = format!(
    "{} WHERE excluded.verified_seq >= asset_creators.verified_seq OR asset_creators.verified_seq is NULL",
    query.sql,
);

        multi_txn
            .execute(query)
            .await
            .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
    }

    // Close out transaction and relinqish the lock.
    multi_txn.commit().await?;

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

    // Do not overwrite changes that happened after decompression (asset_grouping.group_info_seq = 0).
    query.sql = format!(
        "{} WHERE (asset_grouping.group_info_seq != 0 AND excluded.group_info_seq >= asset_grouping.group_info_seq) OR asset_grouping.group_info_seq IS NULL",
        query.sql
    );

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

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
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset_data::ActiveModel {
        id: Set(id.clone()),
        chain_data_mutability: Set(chain_data_mutability),
        chain_data: Set(chain_data),
        metadata_url: Set(metadata_url),
        metadata_mutability: Set(metadata_mutability),
        metadata: Set(metadata),
        slot_updated: Set(slot_updated),
        reindex: Set(reindex),
        raw_name: Set(Some(raw_name)),
        raw_symbol: Set(Some(raw_symbol)),
        base_info_seq: Set(Some(seq)),
        ..Default::default()
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
        "{} WHERE (asset_data.base_info_seq != 0 AND excluded.base_info_seq >= asset_data.base_info_seq) OR asset_data.base_info_seq IS NULL)",
        query.sql
    );
    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn creators_should_be_updated<T>(
    txn: &T,
    id: Vec<u8>,
    seq: i64,
) -> Result<bool, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(asset) = asset::Entity::find_by_id(id).one(txn).await? {
        if let Some(0) = asset.seq {
            return Ok(false);
        }
        if let Some(creators_added_seq) = asset.creators_added_seq {
            if seq < creators_added_seq {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub async fn upsert_asset_with_creators_added_seq<T>(
    txn: &T,
    id: Vec<u8>,
    seq: i64,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset::ActiveModel {
        id: Set(id),
        creators_added_seq: Set(Some(seq)),
        ..Default::default()
    };

    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset::Column::Id)
                .update_columns([asset::Column::CreatorsAddedSeq])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    query.sql = format!(
        "{} WHERE excluded.creators_added_seq >= asset.creators_added_seq OR asset.creators_added_seq IS NULL",
        query.sql
    );

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_creators<T>(
    txn: &T,
    id: Vec<u8>,
    creators: &Vec<Creator>,
    slot_updated: i64,
    seq: i64,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let multi_txn = txn.begin().await?;
    if creators_should_be_updated(&multi_txn, id.clone(), seq).await? {
        // Delete any existing creators.
        asset_creators::Entity::delete_many()
            .filter(Condition::all().add(asset_creators::Column::AssetId.eq(id.clone())))
            .exec(&multi_txn)
            .await?;

        if !creators.is_empty() {
            // Vec to hold base creator information.
            let mut db_creator_infos = Vec::with_capacity(creators.len());

            // Vec to hold info on whether a creator is verified.  This info is protected by `seq` number.
            let mut db_creator_verified_infos = Vec::with_capacity(creators.len());

            // Set to prevent duplicates.
            let mut creators_set = HashSet::new();

            for (i, c) in creators.iter().enumerate() {
                if creators_set.contains(&c.address) {
                    continue;
                }

                db_creator_infos.push(asset_creators::ActiveModel {
                    asset_id: Set(id.clone()),
                    creator: Set(c.address.to_bytes().to_vec()),
                    position: Set(i as i16),
                    share: Set(c.share as i32),
                    slot_updated: Set(Some(slot_updated)),
                    ..Default::default()
                });

                db_creator_verified_infos.push(asset_creators::ActiveModel {
                    asset_id: Set(id.clone()),
                    creator: Set(c.address.to_bytes().to_vec()),
                    verified: Set(c.verified),
                    verified_seq: Set(Some(seq)),
                    ..Default::default()
                });

                creators_set.insert(c.address);
            }

            // This statement will update base information for each creator.
            let query = asset_creators::Entity::insert_many(db_creator_infos)
                .on_conflict(
                    OnConflict::columns([
                        asset_creators::Column::AssetId,
                        asset_creators::Column::Creator,
                    ])
                    .update_columns([
                        asset_creators::Column::Position,
                        asset_creators::Column::Share,
                        asset_creators::Column::SlotUpdated,
                    ])
                    .to_owned(),
                )
                .build(DbBackend::Postgres);
            multi_txn.execute(query).await?;

            // This statement will update whether the creator is verified and the
            // `verified_seq` number.
            let mut query = asset_creators::Entity::insert_many(db_creator_verified_infos)
                .on_conflict(
                    OnConflict::columns([
                        asset_creators::Column::AssetId,
                        asset_creators::Column::Creator,
                    ])
                    .update_columns([
                        asset_creators::Column::Verified,
                        asset_creators::Column::VerifiedSeq,
                    ])
                    .to_owned(),
                )
                .build(DbBackend::Postgres);
            query.sql = format!(
            "{} WHERE excluded.verified_seq >= asset_creators.verified_seq OR asset_creators.verified_seq IS NULL",
            query.sql
        );
            multi_txn.execute(query).await?;
        }

        upsert_asset_with_creators_added_seq(&multi_txn, id, seq).await?;
    }

    // Close out transaction and relinqish the lock.
    multi_txn.commit().await?;

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
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    // Set initial mint info.
    let asset_model = asset::ActiveModel {
        id: Set(id.clone()),
        owner_type: Set(owner_type),
        frozen: Set(frozen),
        specification_version: Set(Some(specification_version)),
        specification_asset_class: Set(Some(specification_asset_class)),
        royalty_target_type: Set(royalty_target_type),
        royalty_target: Set(royalty_target),
        royalty_amount: Set(royalty_amount),
        asset_data: Set(Some(id)),
        slot_updated: Set(Some(slot_updated)),
        base_info_seq: Set(Some(seq)),
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
                    asset::Column::SlotUpdated,
                    asset::Column::BaseInfoSeq,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    // Do not overwrite changes that happened after decompression (asset.seq = 0).
    // Do not overwrite changes from a later Bubblegum instruction.
    query.sql = format!(
        "{} WHERE asset.seq != 0 AND (excluded.base_info_seq >= asset.base_info_seq OR asset.base_info_seq IS NULL)",
        query.sql
    );

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;

    Ok(())
}

pub async fn upsert_asset_authority<T>(
    txn: &T,
    asset_id: Vec<u8>,
    authority: Vec<u8>,
    slot_updated: i64,
    seq: i64,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset_authority::ActiveModel {
        asset_id: Set(asset_id),
        authority: Set(authority),
        seq: Set(seq),
        slot_updated: Set(slot_updated),
        ..Default::default()
    };

    // Do not attempt to modify any existing values:
    // `ON CONFLICT ('asset_id') DO NOTHING`.
    let mut query = asset_authority::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset_authority::Column::AssetId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    // Do not overwrite changes that happened after decompression (asset_authority.seq = 0).
    query.sql = format!("{} WHERE asset_authority.seq != 0", query.sql);

    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;

    Ok(())
}
