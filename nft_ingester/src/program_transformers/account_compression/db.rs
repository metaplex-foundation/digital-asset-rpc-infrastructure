use crate::error::IngesterError;
use digital_asset_types::dao::{backfill_items, cl_audits, cl_items};
use log::{debug, info};
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveValue::Set, ColumnTrait, DbBackend, EntityTrait,
};
use spl_account_compression::events::ChangeLogEventV1;

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
