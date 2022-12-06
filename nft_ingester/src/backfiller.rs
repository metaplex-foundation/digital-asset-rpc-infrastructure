//! Backfiller that fills gaps in trees by detecting gaps in sequence numbers
//! in the `backfill_items` table.  Inspired by backfiller.ts/backfill.ts.
use crate::{
    error::IngesterError, IngesterConfig, DATABASE_LISTENER_CHANNEL_KEY, RPC_COMMITMENT_KEY,
    RPC_URL_KEY,
};
use borsh::BorshDeserialize;
use cadence_macros::statsd_count;
use chrono::Utc;
use digital_asset_types::dao::backfill_items;
use flatbuffers::FlatBufferBuilder;
use futures::stream::FuturesUnordered;
use plerkle_messenger::{Messenger, TRANSACTION_STREAM};
use plerkle_serialization::{
    CompiledInstruction, CompiledInstructionArgs, InnerInstructions, InnerInstructionsArgs,
    Pubkey as FBPubkey, TransactionInfo, TransactionInfoArgs,
};
use sea_orm::{
    entity::*, query::*, sea_query::Expr, DatabaseConnection, DbBackend, DbErr, FromQueryResult,
    SqlxPostgresConnector, TryGetableMany,
};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcBlockConfig, RpcProgramAccountsConfig, RpcAccountInfoConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, MemcmpEncoding, RpcFilterType},
};
use solana_sdk::{
    account::Account,
    commitment_config::{CommitmentConfig, CommitmentLevel},
    pubkey::Pubkey,
};
use solana_transaction_status::{
    EncodedConfirmedBlock, UiInstruction::Compiled, UiRawMessage, UiTransactionEncoding,
    UiTransactionStatusMeta
};
use solana_account_decoder::UiAccountEncoding;
use spl_account_compression::state::{
    CompressionAccountType, ConcurrentMerkleTreeHeader, ConcurrentMerkleTreeHeaderData,
    ConcurrentMerkleTreeHeaderDataV1,
};
use sqlx::{self, postgres::PgListener, Pool, Postgres};
use std::collections::HashMap;
use std::str::FromStr;
use tokio::{time::{self, sleep, Duration}, sync::Semaphore};
// Number of tries to backfill a single tree before marking as "failed".
const NUM_TRIES: i32 = 5;
const TREE_SYNC_INTERVAL: u64 = 30000;
// Constants used for varying delays when failures occur.
const INITIAL_FAILURE_DELAY: u64 = 100;
const MAX_FAILURE_DELAY_MS: u64 = 10_000;

// Account key used to determine if transaction is a simple vote.
const VOTE: &str = "Vote111111111111111111111111111111111111111";

/// Main public entry point for backfiller task.
pub async fn backfiller<T: Messenger>(
    pool: Pool<Postgres>,
    config: IngesterConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let pool_cloned = pool.clone();
            let config_cloned = config.clone();
            let mut tasks = FuturesUnordered::new();

            tasks.push(tokio::spawn(async {
                println!("Backfiller task running");

                let mut backfiller = Backfiller::<T>::new(pool_cloned, config_cloned).await;
                backfiller.run_filler().await;
            }));

            let pool_cloned = pool.clone();
            let config_cloned = config.clone();

            tasks.push(tokio::spawn(async {
                println!("Backfiller task running");

                let mut backfiller = Backfiller::<T>::new(pool_cloned, config_cloned).await;
                backfiller.run_finder().await;
            }));

            for task in tasks {
                let res = task.await;
                match res {
                    Ok(_) => break,
                    Err(err) if err.is_panic() => {
                        statsd_count!("ingester.backfiller.task_panic", 1);
                    }
                    Err(err) => {
                        let err = err.to_string();
                        statsd_count!("ingester.backfiller.task_error", 1, "error" => &err);
                    }
                }
            }
        }
    })
}

/// Struct used when querying for unique trees.
#[derive(Debug, FromQueryResult)]
struct UniqueTree {
    tree: Vec<u8>,
}

/// Struct used when querying for unique trees.
#[derive(Debug, FromQueryResult)]
struct TreeWithSlot {
    tree: Vec<u8>,
    slot: i64,
}

#[derive(Debug, Default, Clone)]
struct MissingTree {
    tree: Pubkey,
    slot: u64,
}

/// Struct used when storing trees to backfill.
struct BackfillTree {
    unique_tree: UniqueTree,
    backfill_from_seq_1: bool,
    slot: u64,
}

impl BackfillTree {
    fn new(unique_tree: UniqueTree, backfill_from_seq_1: bool, slot: u64) -> Self {
        Self {
            unique_tree,
            backfill_from_seq_1,
            slot,
        }
    }
}

/// Struct used when querying the max sequence number of a tree.
#[derive(Debug, FromQueryResult, Clone)]
struct MaxSeqItem {
    seq: i64,
}

/// Struct used when querying for items to backfill.
#[derive(Debug, FromQueryResult, Clone)]
struct SimpleBackfillItem {
    seq: i64,
    slot: i64,
}

/// Struct used to store sequence number gap info for a given tree.
struct GapInfo {
    prev: SimpleBackfillItem,
    curr: SimpleBackfillItem,
}

impl GapInfo {
    fn new(prev: SimpleBackfillItem, curr: SimpleBackfillItem) -> Self {
        Self { prev, curr }
    }
}

/// Main struct used for backfiller task.
struct Backfiller<T: Messenger> {
    db: DatabaseConnection,
    listener: PgListener,
    rpc_client: RpcClient,
    rpc_block_config: RpcBlockConfig,
    messenger: T,
    failure_delay: u64,
}

impl<T: Messenger> Backfiller<T> {
    /// Create a new `Backfiller` struct.
    async fn new(pool: Pool<Postgres>, config: IngesterConfig) -> Self {
        // Create Sea ORM database connection used later for queries.
        let db = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());

        // Connect to database using sqlx and create PgListener.
        let mut listener = sqlx::postgres::PgListener::connect_with(&pool.clone())
            .await
            .map_err(|e| IngesterError::StorageListenerError {
                msg: format!("Could not connect to db for PgListener {e}"),
            })
            .unwrap();

        // Get database listener channel.
        let channel = config
            .database_config
            .get(DATABASE_LISTENER_CHANNEL_KEY)
            .and_then(|u| u.clone().into_string())
            .ok_or(IngesterError::ConfigurationError {
                msg: format!(
                    "Database listener channel missing: {}",
                    DATABASE_LISTENER_CHANNEL_KEY
                ),
            })
            .unwrap();

        // Setup listener on channel.
        listener
            .listen(&channel)
            .await
            .map_err(|e| IngesterError::StorageListenerError {
                msg: format!("Error listening to channel on backfill_items tbl {e}"),
            })
            .unwrap();

        // Get RPC URL.
        let rpc_url = config
            .rpc_config
            .get(RPC_URL_KEY)
            .and_then(|u| u.clone().into_string())
            .ok_or(IngesterError::ConfigurationError {
                msg: format!("RPC URL missing: {}", RPC_URL_KEY),
            })
            .unwrap();

        // Get RPC commitment level.
        let rpc_commitment_level = config
            .rpc_config
            .get(RPC_COMMITMENT_KEY)
            .and_then(|v| v.as_str())
            .ok_or(IngesterError::ConfigurationError {
                msg: format!("RPC commitment level missing: {}", RPC_COMMITMENT_KEY),
            })
            .unwrap();

        // Check if commitment level is valid and create `CommitmentConfig`.
        let rpc_commitment = CommitmentConfig {
            commitment: CommitmentLevel::from_str(rpc_commitment_level)
                .map_err(|_| IngesterError::ConfigurationError {
                    msg: format!("Invalid RPC commitment level: {}", rpc_commitment_level),
                })
                .unwrap(),
        };

        // Create `RpcBlockConfig` used when getting blocks from RPC provider.
        let rpc_block_config = RpcBlockConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(rpc_commitment),
            ..RpcBlockConfig::default()
        };

        // Instantiate RPC client.
        let rpc_client = RpcClient::new_with_commitment(rpc_url, rpc_commitment);

        // Instantiate messenger.
        let mut messenger = T::new(config.messenger_config).await.unwrap();
        messenger.add_stream(TRANSACTION_STREAM).await.unwrap();
        messenger.set_buffer_size(TRANSACTION_STREAM, 5000).await;

        Self {
            db,
            listener,
            rpc_client,
            rpc_block_config,
            messenger,
            failure_delay: INITIAL_FAILURE_DELAY,
        }
    }

    async fn run_finder(&mut self) {
        let mut interval = time::interval(tokio::time::Duration::from_millis(TREE_SYNC_INTERVAL));
        let mut sem = Semaphore::new(1);
        loop {
            interval.tick().await;
            let permit = sem.acquire().await.unwrap();
            println!("Looking for missing trees...");
            match self.get_missing_trees().await {
                Ok(missing_trees) => {
                    let len = missing_trees.len();
                    if len > 0 {
                        statsd_count!("ingester.backfiller.missing_trees", len as i64);
                        let res = self.force_backfill_missing_trees(missing_trees).await;
                        if res.is_ok() {
                            println!(
                                "Missing Trees Found: {} trees scheduled for backfill from 0",
                                len
                            );
                        } else {
                            println!("Error finding missing trees: {}", res.unwrap_err());
                        }
                    }
                }
                Err(e) => {
                    println!("Error getting missing trees: {}", e);
                }
            }
        }
    }
    /// Run the backfiller task.
    async fn run_filler(&mut self) {
        // This is always looping, but if there are no trees to backfill, it will wait for a
        // notification on the db listener channel before continuing.
        loop {
            match self.get_trees_to_backfill().await {
                Ok(backfill_trees) => {
                    if backfill_trees.is_empty() {
                        // If there are no trees to backfill, wait for a notification on the db
                        // listener channel.
                        let _notification = self.listener.recv().await.unwrap();
                    } else {
                        // First just check if we can talk to an RPC provider.
                        match self.rpc_client.get_version().await {
                            Ok(version) => println!("RPC client version {version}"),
                            Err(err) => {
                                println!("RPC client error {err}");
                                self.sleep_and_increase_delay().await;
                                continue;
                            }
                        }

                        for backfill_tree in backfill_trees {
                            for tries in 1..=NUM_TRIES {
                                // Get the tree out of nested structs.
                                let tree = &backfill_tree.unique_tree.tree;
                                let tree_string = bs58::encode(&tree).into_string();
                                println!("Backfilling tree: {tree_string}");

                                // Call different methods based on whether tree needs to be backfilled
                                // completely from seq number 1 or just have any gaps in seq number
                                // filled.
                                let result = if backfill_tree.backfill_from_seq_1 {
                                    self.backfill_tree_from_seq_1(&backfill_tree).await
                                } else {
                                    self.fetch_and_plug_gaps(tree).await
                                };

                                match result {
                                    Ok(opt_max_seq) => {
                                        // Successfully backfilled the tree.  Now clean up database.
                                        self.clean_up_backfilled_tree(
                                            opt_max_seq,
                                            tree,
                                            &tree_string,
                                            tries,
                                        )
                                        .await;
                                        self.reset_delay();
                                        break;
                                    }
                                    Err(err) => {
                                        println!("Failed to fetch and plug gaps for {tree_string}, attempt {tries}");
                                        println!("{err}");
                                    }
                                }

                                if tries == NUM_TRIES {
                                    if let Err(err) = self.mark_tree_as_failed(tree).await {
                                        println!("Error marking tree as failed to backfill: {err}");
                                    }
                                } else {
                                    self.sleep_and_increase_delay().await;
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    // Print error but keep trying.
                    println!("Could not get trees to backfill from db: {err}");
                    self.sleep_and_increase_delay().await;
                }
            }
        }
    }

    async fn force_backfill_missing_trees(
        &mut self,
        missing_trees: Vec<MissingTree>,
    ) -> Result<(), IngesterError> {
        let txn = self.db.begin().await?;
        for tree in missing_trees.into_iter() {
            let item = backfill_items::ActiveModel {
                tree: Set(tree.tree.as_ref().to_vec()),
                seq: Set(0),
                slot: Set(tree.slot as i64),
                force_chk: Set(true),
                backfilled: Set(false),
                failed: Set(false),
                ..Default::default()
            };

            backfill_items::Entity::insert(item).exec(&txn).await?;
        }
        txn.commit().await?;
        Ok(())
    }

    async fn clean_up_backfilled_tree(
        &mut self,
        opt_max_seq: Option<i64>,
        tree: &[u8],
        tree_string: &String,
        tries: i32,
    ) {
        match opt_max_seq {
            Some(max_seq) => {
                // Debug.
                println!("Successfully backfilled tree: {tree_string}, attempt {tries}");

                // Delete extra rows and mark as backfilled.
                match self
                    .delete_extra_rows_and_mark_as_backfilled(tree, max_seq)
                    .await
                {
                    Ok(_) => {
                        // Debug.
                        println!("Successfully deleted rows up to {max_seq}");
                    }
                    Err(err) => {
                        println!("Error deleting rows and marking as backfilled: {err}");
                        if let Err(err) = self.mark_tree_as_failed(tree).await {
                            println!("Error marking tree as failed to backfill: {err}");
                        }
                    }
                }
            }
            None => {
                // Debug.
                println!("Unexpected error, tree was in list, but no rows found for {tree_string}");
                if let Err(err) = self.mark_tree_as_failed(tree).await {
                    println!("Error marking tree as failed to backfill: {err}");
                }
            }
        }
    }

    async fn sleep_and_increase_delay(&mut self) {
        sleep(Duration::from_millis(self.failure_delay)).await;

        // Increase failure delay up to `MAX_FAILURE_DELAY_MS`.
        self.failure_delay = self.failure_delay.saturating_mul(2);
        if self.failure_delay > MAX_FAILURE_DELAY_MS {
            self.failure_delay = MAX_FAILURE_DELAY_MS;
        }
    }

    fn reset_delay(&mut self) {
        self.failure_delay = INITIAL_FAILURE_DELAY;
    }

    async fn get_missing_trees(&self) -> Result<Vec<MissingTree>, IngesterError> {
        let mut all_trees: HashMap<Pubkey, u64> = self.fetch_trees_by_gpa().await?;
        let txn = self.db.begin().await?;
        let get_all_local_trees = Statement::from_string(
            DbBackend::Postgres,
            "SELECT DISTINCT tree FROM cl_items".to_string(),
        );
        let force_chk_trees = txn.query_all(get_all_local_trees).await?;
        for row in force_chk_trees.into_iter() {
            let tree = UniqueTree::from_query_result(&row, "")?;
            let key = &Pubkey::new(&tree.tree);
            if all_trees.contains_key(key) {
                all_trees.remove(key);
            }
        }
        let missing_trees = all_trees
            .into_iter()
            .map(|(k, s)| MissingTree { tree: k, slot: s })
            .collect();
        Ok(missing_trees)
    }

    async fn get_trees_to_backfill(&self) -> Result<Vec<BackfillTree>, DbErr> {
        // Start a db transaction.
        let txn = self.db.begin().await?;

        // Lock the backfill_items table.
        txn.execute(Statement::from_string(
            DbBackend::Postgres,
            "LOCK TABLE backfill_items IN ACCESS EXCLUSIVE MODE".to_string(),
        ))
        .await?;

        // Get trees with the `force_chk` flag set to true (that have not failed and are not locked).
        let force_chk_trees = Statement::from_string(
            DbBackend::Postgres,
            "SELECT DISTINCT backfill_items.tree, backfill_items.slot FROM backfill_items\n\
            WHERE backfill_items.force_chk = TRUE\n\
            AND backfill_items.failed = FALSE\n\
            AND backfill_items.locked = FALSE"
                .to_string(),
        );

        let force_chk_trees: Vec<TreeWithSlot> =
            txn.query_all(force_chk_trees).await.map(|qr| {
                qr.iter()
                    .map(|q| TreeWithSlot::from_query_result(q, "").unwrap())
                    .collect()
            })?;

        println!(
            "Number of force check trees to backfill: {}",
            force_chk_trees.len()
        );

        for tree in force_chk_trees.iter() {
            let stmt = backfill_items::Entity::update_many()
                .col_expr(backfill_items::Column::Locked, Expr::value(true))
                .filter(backfill_items::Column::Tree.eq(&*tree.tree))
                .build(DbBackend::Postgres);

            if let Err(err) = txn.execute(stmt).await {
                println!(
                    "Error marking tree {} as locked: {}",
                    bs58::encode(&tree.tree).into_string(),
                    err
                );
                return Err(err);
            }
        }

        // Get trees with multiple rows from `backfill_items` table (that have not failed and are not locked).
        let multi_row_trees = Statement::from_string(
            DbBackend::Postgres,
            "SELECT backfill_items.tree, max(backfill_items.slot) as slot FROM backfill_items\n\
            WHERE backfill_items.failed = FALSE
            AND backfill_items.locked = FALSE\n\
            GROUP BY backfill_items.tree\n\
            HAVING COUNT(*) > 1"
                .to_string(),
        );

        let multi_row_trees: Vec<TreeWithSlot> =
            txn.query_all(multi_row_trees).await.map(|qr| {
                qr.iter()
                    .map(|q| TreeWithSlot::from_query_result(q, "").unwrap())
                    .collect()
            })?;

        println!(
            "Number of multi-row trees to backfill {}",
            multi_row_trees.len()
        );

        for tree in multi_row_trees.iter() {
            let stmt = backfill_items::Entity::update_many()
                .col_expr(backfill_items::Column::Locked, Expr::value(true))
                .filter(backfill_items::Column::Tree.eq(&*tree.tree))
                .build(DbBackend::Postgres);

            if let Err(err) = txn.execute(stmt).await {
                println!(
                    "Error marking tree {} as locked: {}",
                    bs58::encode(&tree.tree).into_string(),
                    err
                );
                return Err(err);
            }
        }

        // Close out transaction and relinqish the lock.
        txn.commit().await?;

        // Convert force check trees Vec of `UniqueTree` to a Vec of `BackfillTree` (which contain extra info).
        let mut trees: Vec<BackfillTree> = force_chk_trees
            .into_iter()
            .map(|tree| BackfillTree::new(UniqueTree { tree: tree.tree }, true, tree.slot as u64))
            .collect();

        // Convert multi-row trees Vec of `UniqueTree` to a Vec of `BackfillTree` (which contain extra info).
        let mut multi_row_trees: Vec<BackfillTree> = multi_row_trees
            .into_iter()
            .map(|tree| BackfillTree::new(UniqueTree { tree: tree.tree }, false, tree.slot as u64))
            .collect();

        trees.append(&mut multi_row_trees);

        Ok(trees)
    }

    async fn backfill_tree_from_seq_1(
        &mut self,
        btree: &BackfillTree,
    ) -> Result<Option<i64>, IngesterError> {
        let slot =
            self.rpc_client.get_slot().await.map_err(|e| {
                IngesterError::RpcGetDataError(format!("Failed to get slot: {:?}", e))
            })?;
        let gap = GapInfo::new(
            SimpleBackfillItem {
                seq: 0,
                slot: btree.slot as i64,
            },
            SimpleBackfillItem {
                seq: 0,
                slot: slot as i64,
            },
        );
        println!("start slot {:?}, end slot {:?}", btree.slot, slot);
        self.plug_gap(&gap, &btree.unique_tree.tree).await?;
        Ok(Some(0))
    }

    async fn get_max_seq(&self, tree: &[u8]) -> Result<Option<i64>, DbErr> {
        let query = backfill_items::Entity::find()
            .select_only()
            .column(backfill_items::Column::Seq)
            .filter(backfill_items::Column::Tree.eq(tree))
            .order_by_desc(backfill_items::Column::Seq)
            .limit(1)
            .build(DbBackend::Postgres);

        let start_seq_vec = MaxSeqItem::find_by_statement(query).all(&self.db).await?;

        Ok(start_seq_vec.last().map(|row| row.seq))
    }

    async fn clear_force_chk_flag(&self, tree: &[u8]) -> Result<UpdateResult, DbErr> {
        backfill_items::Entity::update_many()
            .col_expr(backfill_items::Column::ForceChk, Expr::value(false))
            .filter(backfill_items::Column::Tree.eq(tree))
            .exec(&self.db)
            .await
    }

    async fn fetch_trees_by_gpa(&self) -> Result<HashMap<Pubkey, u64>, IngesterError> {
        let config = RpcProgramAccountsConfig {
            filters: Some(vec![RpcFilterType::Memcmp(Memcmp {
                encoding: Some(MemcmpEncoding::Binary),
                offset: 0,
                bytes: MemcmpEncodedBytes::Bytes(vec![1u8]),
            })]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                ..RpcAccountInfoConfig::default()
            },
            ..RpcProgramAccountsConfig::default()
        };
        let results: Vec<(Pubkey, Account)> = self
            .rpc_client
            .get_program_accounts_with_config(&spl_account_compression::id(), config)
            .await
            .map_err(|e| IngesterError::RpcGetDataError(e.to_string()))?;
        let mut list = HashMap::with_capacity(results.len());
        for r in results.into_iter() {
            let (pubkey, account) = r;
            println!("found rpc tree pubkey: {}", pubkey);
            let mut sl = account.data.as_slice();
            let tree_config: ConcurrentMerkleTreeHeader = ConcurrentMerkleTreeHeader::deserialize(&mut sl)
                    .map_err(|e| IngesterError::RpcGetDataError(e.to_string()))?;
            list.insert(pubkey, tree_config.get_creation_slot());
        }
        Ok(list)
    }

    // Similar to `fetchAndPlugGaps()` in `backfiller.ts`.
    async fn fetch_and_plug_gaps(&mut self, tree: &[u8]) -> Result<Option<i64>, IngesterError> {
        let (opt_max_seq, gaps) = self.get_missing_data(tree).await?;

        // Similar to `plugGapsBatched()` in `backfiller.ts` (although not batched).
        for gap in gaps.iter() {
            // Similar to `plugGaps()` in `backfiller.ts`.
            self.plug_gap(gap, tree).await?;
        }

        Ok(opt_max_seq)
    }

    // Similar to `getMissingData()` in `db.ts`.
    async fn get_missing_data(&self, tree: &[u8]) -> Result<(Option<i64>, Vec<GapInfo>), DbErr> {
        // Get the maximum sequence number that has been backfilled, and use
        // that for the starting sequence number for backfilling.
        let query = backfill_items::Entity::find()
            .select_only()
            .column(backfill_items::Column::Seq)
            .filter(backfill_items::Column::Tree.eq(tree))
            .filter(backfill_items::Column::Backfilled.eq(true))
            .order_by_desc(backfill_items::Column::Seq)
            .limit(1)
            .build(DbBackend::Postgres);

        let start_seq_vec = MaxSeqItem::find_by_statement(query).all(&self.db).await?;
        let start_seq = if let Some(seq) = start_seq_vec.last().map(|row| row.seq) {
            seq
        } else {
            0
        };

        // Get all rows for the tree that have not yet been backfilled.
        let mut query = backfill_items::Entity::find()
            .select_only()
            .column(backfill_items::Column::Seq)
            .column(backfill_items::Column::Slot)
            .filter(backfill_items::Column::Seq.gte(start_seq))
            .filter(backfill_items::Column::Tree.eq(tree))
            .order_by_asc(backfill_items::Column::Seq)
            .build(DbBackend::Postgres);

        query.sql = query.sql.replace("SELECT", "SELECT DISTINCT");
        let rows = SimpleBackfillItem::find_by_statement(query)
            .all(&self.db)
            .await?;
        let mut gaps = vec![];

        // Look at each pair of subsequent rows, looking for a gap in sequence number.
        for (prev, curr) in rows.iter().zip(rows.iter().skip(1)) {
            if curr.seq == prev.seq {
                let message = format!(
                    "Error in DB, identical sequence numbers with different slots: {}, {}",
                    prev.slot, curr.slot
                );
                println!("{}", message);
                return Err(DbErr::Custom(message));
            } else if curr.seq - prev.seq > 1 {
                gaps.push(GapInfo::new(prev.clone(), curr.clone()));
            }
        }

        // Get the max sequence number if any rows were returned from the query.
        let opt_max_seq = rows.last().map(|row| row.seq);

        Ok((opt_max_seq, gaps))
    }

    // Similar to `plugGaps()` in `backfiller.ts`.
    async fn plug_gap(&mut self, gap: &GapInfo, tree: &[u8]) -> Result<(), IngesterError> {
        // TODO: This needs to make sure all slots are available otherwise it will partially
        // fail and redo the whole backfill process.  So for now checking the max block before
        // looping as a quick workaround.
        let _ = self
            .rpc_client
            .get_block(gap.curr.slot as u64)
            .await
            .map_err(|e| IngesterError::RpcGetDataError(e.to_string()))?;

        for slot in gap.prev.slot..gap.curr.slot {
            let block_data = EncodedConfirmedBlock::from(
                self.rpc_client
                    .get_block_with_config(slot as u64, self.rpc_block_config)
                    .await
                    .map_err(|e| IngesterError::RpcGetDataError(e.to_string()))?,
            );

            // Debug.
            println!("num txs: {}", block_data.transactions.len());
            for tx in block_data.transactions {
                // Debug.
                //println!("TX from RPC provider:");
                //println!("{:#?}", tx);

                // See if transaction has an error.
                let meta = if let Some(meta) = &tx.meta {
                    if let Some(err) = &meta.err {
                        println!("Transaction has error: {err}");
                        continue;
                    }
                    meta
                } else {
                    println!("Unexpected, EncodedTransactionWithStatusMeta struct has no metadata");
                    continue;
                };

                // Get `UiTransaction` out of `EncodedTransactionWithStatusMeta`.
                let ui_transaction = match tx.transaction {
                    solana_transaction_status::EncodedTransaction::Json(ref ui_transaction) => {
                        ui_transaction
                    }
                    _ => {
                        return Err(IngesterError::RpcDataUnsupportedFormat(
                            "EncodedTransaction".to_string(),
                        ));
                    }
                };

                // See if transaction is a vote.
                let ui_raw_message = match &ui_transaction.message {
                    solana_transaction_status::UiMessage::Raw(ui_raw_message) => {
                        if ui_raw_message.account_keys.iter().any(|key| key == VOTE) {
                            continue;
                        } else {
                            ui_raw_message
                        }
                    }
                    _ => {
                        return Err(IngesterError::RpcDataUnsupportedFormat(
                            "UiMessage".to_string(),
                        ));
                    }
                };
                let sig = ui_transaction.signatures[0].to_string();
                // Filter out transactions that don't have to do with the tree we are interested in or
                // the Bubblegum program.
                let tree = bs58::encode(tree).into_string();
                let bubblegum = blockbuster::programs::bubblegum::program_id().to_string();
                if ui_raw_message
                    .account_keys
                    .iter()
                    .all(|pk| *pk != tree && *pk != bubblegum)
                {
                    // Debug.
                    println!("Skipping tx unrelated to tree or bubblegum PID {:?}", sig);
                    continue;
                }
                
                // Serialize data.
                let builder = FlatBufferBuilder::new();
                
                println!("Serializing transaction in backfiller {}", sig);
                let builder = serialize_transaction(
                    builder,
                    sig,
                    meta,
                    ui_raw_message,
                    slot.try_into().unwrap(),
                )?;

                // Debug.
                println!("Putting data into Redis");
                // Put data into Redis.
                self.messenger
                    .send(TRANSACTION_STREAM, builder.finished_data())
                    .await?;
            }
        }

        Ok(())
    }

    async fn delete_extra_rows_and_mark_as_backfilled(
        &self,
        tree: &[u8],
        max_seq: i64,
    ) -> Result<(), DbErr> {
        // Debug.
        let test_items = backfill_items::Entity::find()
            .filter(backfill_items::Column::Tree.eq(tree))
            .all(&self.db)
            .await?;
        println!("Count of items before delete: {}", test_items.len());
        for item in test_items {
            println!("Seq ID {}", item.seq);
        }

        // Delete all rows in the `backfill_items` table for a specified tree, except for the row with
        // the caller-specified max seq number.  One row for each tree must remain so that gaps can be
        // detected after subsequent inserts.
        backfill_items::Entity::delete_many()
            .filter(backfill_items::Column::Tree.eq(tree))
            .filter(backfill_items::Column::Seq.ne(max_seq))
            .exec(&self.db)
            .await?;

        // Remove any duplicates that have the caller-specified max seq number.  This happens when
        // a transaction that was already handled is replayed during backfilling.
        let items = backfill_items::Entity::find()
            .filter(backfill_items::Column::Tree.eq(tree))
            .filter(backfill_items::Column::Seq.eq(max_seq))
            .all(&self.db)
            .await?;

        if items.len() > 1 {
            for item in items.iter().skip(1) {
                backfill_items::Entity::delete_by_id(item.id)
                    .exec(&self.db)
                    .await?;
            }
        }

        // Mark remaining row as backfilled so future backfilling can start above this sequence number.
        self.mark_tree_as_backfilled(tree).await?;

        // Clear the `force_chk` flag if it was set.
        self.clear_force_chk_flag(tree).await?;

        // Unlock tree.
        self.unlock_tree(tree).await?;

        // Debug.
        let test_items = backfill_items::Entity::find()
            .filter(backfill_items::Column::Tree.eq(tree))
            .all(&self.db)
            .await?;
        println!("Count of items after delete: {}", test_items.len());
        for item in test_items {
            println!("Seq ID {}", item.seq);
        }

        Ok(())
    }

    async fn mark_tree_as_backfilled(&self, tree: &[u8]) -> Result<(), DbErr> {
        backfill_items::Entity::update_many()
            .col_expr(backfill_items::Column::Backfilled, Expr::value(true))
            .filter(backfill_items::Column::Tree.eq(tree))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn mark_tree_as_failed(&self, tree: &[u8]) -> Result<(), DbErr> {
        backfill_items::Entity::update_many()
            .col_expr(backfill_items::Column::Failed, Expr::value(true))
            .filter(backfill_items::Column::Tree.eq(tree))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn unlock_tree(&self, tree: &[u8]) -> Result<(), DbErr> {
        backfill_items::Entity::update_many()
            .col_expr(backfill_items::Column::Locked, Expr::value(false))
            .filter(backfill_items::Column::Tree.eq(tree))
            .exec(&self.db)
            .await?;

        Ok(())
    }
}

fn serialize_transaction<'a>(
    mut builder: FlatBufferBuilder<'a>,
    signature: String,
    meta: &UiTransactionStatusMeta,
    ui_raw_message: &UiRawMessage,
    slot: u64,
) -> Result<FlatBufferBuilder<'a>, IngesterError> {
    // Serialize account keys.
    let account_keys = &ui_raw_message.account_keys;
    let account_keys_len = account_keys.len();

    let account_keys = if account_keys_len > 0 {
        let mut account_keys_fb_vec = Vec::with_capacity(account_keys_len);
        for key in account_keys.iter() {
            let key = Pubkey::from_str(key)
                .map_err(|e| IngesterError::SerializatonError(e.to_string()))?;
            account_keys_fb_vec.push(FBPubkey(key.to_bytes()));
        }
        Some(builder.create_vector(&account_keys_fb_vec))
    } else {
        None
    };

    // Serialize log messages.
    let log_messages = if let Some(log_messages) = meta.log_messages.as_ref() {
        let mut log_messages_fb_vec = Vec::with_capacity(log_messages.len());
        for message in log_messages {
            log_messages_fb_vec.push(builder.create_string(message));
        }
        Some(builder.create_vector(&log_messages_fb_vec))
    } else {
        None
    };

    // Serialize inner instructions.
    let inner_instructions = if let Some(inner_instructions_vec) = meta.inner_instructions.as_ref()
    {
        let mut overall_fb_vec = Vec::with_capacity(inner_instructions_vec.len());
        for inner_instructions in inner_instructions_vec.iter() {
            let index = inner_instructions.index;
            let mut instructions_fb_vec = Vec::with_capacity(inner_instructions.instructions.len());
            for ui_instruction in inner_instructions.instructions.iter() {
                if let Compiled(ui_compiled_instruction) = ui_instruction {
                    let program_id_index = ui_compiled_instruction.program_id_index;
                    let accounts = Some(builder.create_vector(&ui_compiled_instruction.accounts));
                    let data = bs58::decode(&ui_compiled_instruction.data)
                        .into_vec()
                        .map_err(|e| IngesterError::SerializatonError(e.to_string()))?;
                    let data = Some(builder.create_vector(&data));
                    instructions_fb_vec.push(CompiledInstruction::create(
                        &mut builder,
                        &CompiledInstructionArgs {
                            program_id_index,
                            accounts,
                            data,
                        },
                    ));
                }
            }

            let instructions = Some(builder.create_vector(&instructions_fb_vec));
            overall_fb_vec.push(InnerInstructions::create(
                &mut builder,
                &InnerInstructionsArgs {
                    index,
                    instructions,
                },
            ))
        }

        Some(builder.create_vector(&overall_fb_vec))
    } else {
        None
    };

    // Serialize outer instructions.
    let outer_instructions = &ui_raw_message.instructions;
    let outer_instructions = if !outer_instructions.is_empty() {
        let mut instructions_fb_vec = Vec::with_capacity(outer_instructions.len());
        for ui_compiled_instruction in outer_instructions.iter() {
            let program_id_index = ui_compiled_instruction.program_id_index;
            let accounts = Some(builder.create_vector(&ui_compiled_instruction.accounts));
            let data = bs58::decode(&ui_compiled_instruction.data)
                .into_vec()
                .map_err(|e| IngesterError::SerializatonError(e.to_string()))?;
            let data = Some(builder.create_vector(&data));
            instructions_fb_vec.push(CompiledInstruction::create(
                &mut builder,
                &CompiledInstructionArgs {
                    program_id_index,
                    accounts,
                    data,
                },
            ));
        }
        Some(builder.create_vector(&instructions_fb_vec))
    } else {
        None
    };

    // Serialize everything into Transaction Info table.
    let seen_at = Utc::now();
    let sig_db = builder.create_string(&signature);
    let transaction_info = TransactionInfo::create(
        &mut builder,
        &TransactionInfoArgs {
            is_vote: false,
            account_keys,
            log_messages,
            inner_instructions,
            outer_instructions,
            slot,
            seen_at: seen_at.timestamp_millis(),
            slot_index: None,
            signature: Some(sig_db),
        },
    );

    // Finalize buffer and return to caller.
    builder.finish(transaction_info, None);
    Ok(builder)
}
