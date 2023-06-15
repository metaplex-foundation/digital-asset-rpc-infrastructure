use crate::error::IngesterError;
use digital_asset_types::dao::{asset, asset_creators, backfill_items, cl_audits, cl_items};
use log::{debug, info, warn};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ColumnTrait, DbBackend, DbErr, EntityTrait,
};
use spl_account_compression::events::ChangeLogEventV1;

use std::convert::From;

pub async fn save_changelog_event<'c, T>(
    change_log_event: &ChangeLogEventV1,
    slot: u64,
    txn_id: &str,
    txn: &T,
) -> Result<u64, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    insert_change_log(change_log_event, slot, txn_id, txn).await?;
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

        //let mut audit_item: cl_audits::ActiveModel = item.clone().into();
        //audit_item.tx = Set(txn_id.to_string());

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
        /* LK: Removing the filling param for now because it's not used anywhere and gives
         * an incorrect view of what might be going on.
         * if !filling {*/
        query.sql = format!("{} WHERE excluded.seq > cl_items.seq", query.sql);
        /*} */
        txn.execute(query)
            .await
            .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

        // Insert the audit item after the insert into cl_items have been completed
        // Disabling cl_audits for now
        //cl_audits::Entity::insert(audit_item).exec(txn).await?;
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

pub async fn update_asset<T>(
    txn: &T,
    id: Vec<u8>,
    seq: Option<u64>,
    model: asset::ActiveModel,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let update_one = if let Some(seq) = seq {
        asset::Entity::update(model).filter(
            Condition::all()
                .add(asset::Column::Id.eq(id.clone()))
                .add(asset::Column::Seq.lte(seq)),
        )
    } else {
        asset::Entity::update(model).filter(asset::Column::Id.eq(id.clone()))
    };

    match update_one.exec(txn).await {
        Ok(_) => Ok(()),
        Err(err) => match err {
            DbErr::RecordNotFound(ref s) => {
                if s.contains("None of the database rows are affected") {
                    warn!(
                        "Update failed. No asset found for id {}.",
                        bs58::encode(id).into_string()
                    );
                    Ok(())
                } else {
                    Err(IngesterError::from(err))
                }
            }
            _ => Err(IngesterError::from(err)),
        },
    }
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
