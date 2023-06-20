use crate::error::IngesterError;
use digital_asset_types::dao::{asset, asset_creators, backfill_items, cl_items};
use log::{debug, info};
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveValue::Set, ColumnTrait, DbBackend, EntityTrait,
};
use spl_account_compression::events::ChangeLogEventV1;

pub async fn save_changelog_event<'c, T>(
    change_log_event: &ChangeLogEventV1,
    slot: u64,
    txn: &T,
) -> Result<u64, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    insert_change_log(change_log_event, slot, txn).await?;
    Ok(change_log_event.seq)
}

fn node_idx_to_leaf_idx(index: i64, tree_height: u32) -> i64 {
    index - 2i64.pow(tree_height)
}

pub async fn insert_change_log<'c, T>(
    change_log_event: &ChangeLogEventV1,
    slot: u64,
    txn: &T,
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
            "seq {}, index {} level {}, node {:?}",
            change_log_event.seq,
            p.index,
            i,
            bs58::encode(p.node).into_string()
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

pub async fn upsert_asset_with_leaf_info<T>(
    txn: &T,
    id: Vec<u8>,
    leaf: Option<Vec<u8>>,
    seq: Option<i64>,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = asset::ActiveModel {
        id: Set(id),
        leaf: Set(leaf),
        seq: Set(seq),
        ..Default::default()
    };

    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset::Column::Id)
                .update_columns([asset::Column::Leaf, asset::Column::Seq])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    query.sql = format!(
        "{} WHERE (asset.was_decompressed = 0) AND (excluded.seq > asset.seq OR asset.seq IS NULL)",
        query.sql
    );

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
        "{} WHERE excluded.owner_delegate_seq > asset.owner_delegate_seq OR asset.owner_delegate_seq IS NULL",
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
        //was_decompressed: Set(was_decompressed),
        ..Default::default()
    };

    // TODO I think we can re-run it, it would be indexing same data but that's fine.
    // // Do not run this command if the asset is already marked as
    // // decompressed.
    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::Compressed,
                    asset::Column::Compressible,
                    asset::Column::Supply,
                    asset::Column::SupplyMint,
                    //asset::Column::WasDecompressed,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    query.sql = format!("{} WHERE asset.was_decompressed = 0", query.sql);
    txn.execute(query).await?;

    Ok(())
}

pub async fn update_creator<T>(
    txn: &T,
    asset_id: Vec<u8>,
    creator: Vec<u8>,
    seq: u64,
    model: asset_creators::ActiveModel,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    // Using `update_many` to avoid having to supply the primary key as well within `model`.
    // We still effectively end up updating a single row at most, which is uniquely identified
    // by the `(asset_id, creator)` pair. Is there any reason why we should not use
    // `update_many` here?
    let update = asset_creators::Entity::update_many()
        .filter(
            Condition::all()
                .add(asset_creators::Column::AssetId.eq(asset_id))
                .add(asset_creators::Column::Creator.eq(creator))
                .add(asset_creators::Column::Seq.lte(seq)),
        )
        .set(model);

    update.exec(txn).await.map_err(IngesterError::from)?;

    Ok(())
}
