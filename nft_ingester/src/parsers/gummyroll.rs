use {
    crate::{
        error::IngesterError, events::handle_event, utils::filter_events_from_logs,
        InstructionBundle, ProgramHandler, ProgramHandlerConfig,
    },
    async_trait::async_trait,
    digital_asset_types::dao::{backfill_items, cl_items},
    gummyroll::state::ChangeLogEvent,
    lazy_static::lazy_static,
    sea_orm::{
        entity::*, query::*, sea_query::OnConflict, DatabaseConnection, DatabaseTransaction,
        DbBackend, SqlxPostgresConnector, TransactionTrait,
    },
    serde::Deserialize,
    solana_sdk::pubkey::Pubkey,
    solana_sdk::pubkeys,
    sqlx::{self, Pool, Postgres},
};

#[derive(Debug, Deserialize)]
pub struct CLRecord {
    node_idx: u32,
    level: u32,
    seq: u32,
    hash: String,
}

pubkeys!(
    GummyRollProgramID,
    "GRoLLzvxpxxu2PGNJMMeZPyMxjAUH9pKqxGXV9DGiceU"
);

pub struct GummyRollHandler {
    id: Pubkey,
    storage: DatabaseConnection,
}

#[async_trait]
impl ProgramHandler for GummyRollHandler {
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
        let _ = handle_gummyroll_instruction(&bundle.instruction_logs, bundle.slot, &self.storage)
            .await?;
        Ok(())
    }
}

impl GummyRollHandler {
    pub fn new(pool: Pool<Postgres>) -> Self {
        GummyRollHandler {
            id: GummyRollProgramID(),
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
        }
    }
}

pub fn get_gummy_roll_events(logs: &Vec<&str>) -> Result<Vec<ChangeLogEvent>, IngesterError> {
    let change_log_event_vec = if let Ok(change_log_event_vec) = filter_events_from_logs(logs) {
        change_log_event_vec
    } else {
        println!("Could not find emitted program data");
        return Err(IngesterError::ChangeLogEventMalformed);
    };
    let mut events = vec![];
    // Parse each change log event found in logs
    for event in change_log_event_vec {
        if let Ok(change_log_event) = handle_event(event) {
            events.push(change_log_event);
        } else {
            continue;
        };
    }
    Ok(events)
}

pub async fn save_changelog_events(
    gummy_roll_events: Vec<ChangeLogEvent>,
    slot: u64,
    txn: &DatabaseTransaction,
) -> Result<Vec<u64>, IngesterError> {
    let mut seq_nums = Vec::with_capacity(gummy_roll_events.len());
    for change_log_event in gummy_roll_events {
        gummyroll_change_log_event_to_database(&change_log_event, slot, txn, false).await?;

        seq_nums.push(change_log_event.seq);
    }
    Ok(seq_nums)
}

pub async fn handle_gummyroll_instruction(
    logs: &Vec<&str>,
    slot: u64,
    db: &DatabaseConnection,
) -> Result<Vec<u64>, IngesterError> {
    // map to owned vec to avoid static lifetime issues, instead of moving logs into Box
    let events = get_gummy_roll_events(logs)?;
    db.transaction::<_, _, IngesterError>(|txn| {
        Box::pin(async move { save_changelog_events(events, slot, txn).await })
    })
    .await
    .map_err(Into::into)
}

fn node_idx_to_leaf_idx(index: i64, tree_height: u32) -> i64 {
    index - 2i64.pow(tree_height)
}

pub async fn gummyroll_change_log_event_to_database(
    change_log_event: &ChangeLogEvent,
    slot: u64,
    txn: &DatabaseTransaction,
    filling: bool,
) -> Result<(), IngesterError> {
    let mut i: i64 = 0;
    let depth = change_log_event.path.len() - 1;
    let tree_id = change_log_event.id.as_ref();
    for p in change_log_event.path.iter() {
        let node_idx = p.index as i64;
        println!(
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
                    .update_columns([cl_items::Column::Hash, cl_items::Column::Seq])
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        if !filling {
            query.sql = format!("{} WHERE excluded.seq > cl_items.seq", query.sql);
        }
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
        let force_chk = rows.len() == 0 && change_log_event.seq > 1;

        println!("Adding to backfill_items table at level {}", i - 1);
        let item = backfill_items::ActiveModel {
            tree: Set(tree_id.to_vec()),
            seq: Set(change_log_event.seq as i64),
            slot: Set(slot as i64),
            force_chk: Set(Some(force_chk)),
            backfilled: Set(Some(false)),
            ..Default::default()
        };

        backfill_items::Entity::insert(item).exec(txn).await?;
    }

    Ok(())
    //TODO -> set maximum size of path and break into multiple statements
}
