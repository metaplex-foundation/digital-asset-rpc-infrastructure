//! Backfiller that fills gaps in trees by detecting gaps in sequence numbers
//! in the `backfill_items` table.  Inspired by backfiller.ts/backfill.ts.

use borsh::BorshDeserialize;
use cadence_macros::{is_global_default_set, statsd_count, statsd_gauge};
use chrono::Utc;
use digital_asset_types::dao::backfill_items;
use flatbuffers::FlatBufferBuilder;
use futures::{stream::FuturesUnordered, StreamExt};
use log::{debug, error, info};
use plerkle_messenger::{Messenger, TRANSACTION_BACKFILL_STREAM};
use plerkle_serialization::serializer::seralize_encoded_transaction_with_status;

use sea_orm::{
    entity::*, query::*, sea_query::Expr, DatabaseConnection, DbBackend, DbErr, FromQueryResult,
    SqlxPostgresConnector,
};
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_config::{RpcAccountInfoConfig, RpcBlockConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{
    account::Account,
    commitment_config::{CommitmentConfig, CommitmentLevel},
    pubkey::Pubkey,
    signature::Signature,
    slot_history::Slot,
};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedBlock,
    EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding,
};
use spl_account_compression::state::{
    merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
};
use sqlx::{self, Pool, Postgres};
use std::{
    cmp,
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Arc,
};
use stretto::{AsyncCache, AsyncCacheBuilder};
use tokio::{
    sync::Semaphore,
    task::JoinSet,
    time::{self, sleep, Duration},
};

use crate::{
    config::{IngesterConfig, DATABASE_LISTENER_CHANNEL_KEY, RPC_COMMITMENT_KEY, RPC_URL_KEY},
    error::IngesterError,
    metric,
};
// Number of tries to backfill a single tree before marking as "failed".
const NUM_TRIES: i32 = 5;
const TREE_SYNC_INTERVAL: u64 = 60;
const MAX_BACKFILL_CHECK_WAIT: u64 = 1000;
// Constants used for varying delays when failures occur.
const INITIAL_FAILURE_DELAY: u64 = 100;
const MAX_FAILURE_DELAY_MS: u64 = 10_000;
const BLOCK_CACHE_SIZE: usize = 300_000;
const MAX_CACHE_COST: i64 = 32;
const BLOCK_CACHE_DURATION: u64 = 172800;

#[allow(dead_code)]
struct SlotSeq(u64, u64);
/// Main public entry point for backfiller task.
pub fn setup_backfiller<T: Messenger>(
    pool: Pool<Postgres>,
    config: IngesterConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let pool_cloned = pool.clone();
            let config_cloned = config.clone();
            let block_cache = Arc::new(
                AsyncCacheBuilder::new(BLOCK_CACHE_SIZE, MAX_CACHE_COST)
                    .set_ignore_internal_cost(true)
                    .finalize(tokio::spawn)
                    .expect("failed to create cache"),
            );
            let mut tasks = JoinSet::new();
            let bc = Arc::clone(&block_cache);
            tasks.spawn(async move {
                info!("Backfiller filler running");
                let mut backfiller = Backfiller::<T>::new(pool_cloned, config_cloned, &bc).await;
                backfiller.run_filler().await;
            });

            let pool_cloned = pool.clone();
            let config_cloned = config.clone();
            let bc = Arc::clone(&block_cache);
            tasks.spawn(async move {
                info!("Backfiller finder running");
                let mut backfiller = Backfiller::<T>::new(pool_cloned, config_cloned, &bc).await;
                backfiller.run_finder().await;
            });

            while let Some(task) = tasks.join_next().await {
                match task {
                    Ok(_) => break,
                    Err(err) if err.is_panic() => {
                        metric! {
                            statsd_count!("ingester.backfiller.task_panic", 1);
                        }
                    }
                    Err(err) => {
                        let err = err.to_string();
                        metric! {
                            statsd_count!("ingester.backfiller.task_error", 1, "error" => &err);
                        }
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
    #[allow(dead_code)]
    slot: u64,
}

impl BackfillTree {
    const fn new(unique_tree: UniqueTree, backfill_from_seq_1: bool, slot: u64) -> Self {
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
#[derive(Debug)]
struct GapInfo {
    prev: SimpleBackfillItem,
    curr: SimpleBackfillItem,
}

impl GapInfo {
    const fn new(prev: SimpleBackfillItem, curr: SimpleBackfillItem) -> Self {
        Self { prev, curr }
    }
}

/// Main struct used for backfiller task.
struct Backfiller<'a, T: Messenger> {
    config: IngesterConfig,
    db: DatabaseConnection,
    rpc_client: RpcClient,
    rpc_block_config: RpcBlockConfig,
    messenger: T,
    failure_delay: u64,
    cache: &'a AsyncCache<String, EncodedConfirmedBlock>,
}

impl<'a, T: Messenger> Backfiller<'a, T> {
    /// Create a new `Backfiller` struct.
    async fn new(
        pool: Pool<Postgres>,
        config: IngesterConfig,
        cache: &'a AsyncCache<String, EncodedConfirmedBlock>,
    ) -> Backfiller<'a, T> {
        // Create Sea ORM database connection used later for queries.
        let db = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());

        // Get database listener channel.
        let _channel = config
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
            encoding: Some(UiTransactionEncoding::Base64),
            commitment: Some(rpc_commitment),
            max_supported_transaction_version: Some(0),
            ..RpcBlockConfig::default()
        };

        // Instantiate RPC client.
        let rpc_client = RpcClient::new_with_commitment(rpc_url, rpc_commitment);

        // Instantiate messenger.
        let mut messenger = T::new(config.get_messneger_client_config()).await.unwrap();
        messenger
            .add_stream(TRANSACTION_BACKFILL_STREAM)
            .await
            .unwrap();
        messenger
            .set_buffer_size(TRANSACTION_BACKFILL_STREAM, 10_000_000)
            .await;

        Self {
            config,
            db,
            rpc_client,
            rpc_block_config,
            messenger,
            failure_delay: INITIAL_FAILURE_DELAY,
            cache,
        }
    }

    async fn run_finder(&mut self) {
        let mut interval = time::interval(tokio::time::Duration::from_secs(TREE_SYNC_INTERVAL));
        let sem = Semaphore::new(1);
        loop {
            interval.tick().await;
            let _permit = sem.acquire().await.unwrap();

            debug!("Looking for missing trees...");

            let missing = self.get_missing_trees(&self.db).await;
            match missing {
                Ok(missing_trees) => {
                    let txn = self.db.begin().await.unwrap();
                    let len = missing_trees.len();
                    metric! {
                        statsd_gauge!("ingester.backfiller.missing_trees", len as f64);
                    }
                    debug!("Found {} missing trees", len);
                    if len > 0 {
                        let res = self.force_backfill_missing_trees(missing_trees, &txn).await;

                        let res2 = txn.commit().await;
                        match (res, res2) {
                            (Ok(_), Ok(_)) => {
                                debug!("Set {} trees to backfill from 0", len);
                            }
                            (Err(e), _) => {
                                error!("Error setting trees to backfill from 0: {}", e);
                            }
                            (_, Err(e)) => {
                                error!("Error setting trees to backfill from 0: error committing transaction: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Error getting missing trees: {}", e);
                }
            }
        }
    }
    /// Run the backfiller task.
    async fn run_filler(&mut self) {
        let mut interval =
            time::interval(tokio::time::Duration::from_millis(MAX_BACKFILL_CHECK_WAIT));
        loop {
            interval.tick().await;
            match self.get_trees_to_backfill().await {
                Ok(backfill_trees) => {
                    if !backfill_trees.is_empty() {
                        for backfill_tree in backfill_trees {
                            for tries in 1..=NUM_TRIES {
                                // Get the tree out of nested structs.
                                let tree = &backfill_tree.unique_tree.tree;
                                let tree_string = bs58::encode(&tree).into_string();
                                info!("Backfilling tree: {tree_string}");
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
                                        error!("Failed to fetch and plug gaps for {tree_string}, attempt {tries}");
                                        error!("{err}");
                                    }
                                }

                                if tries == NUM_TRIES {
                                    if let Err(err) = self.mark_tree_as_failed(tree).await {
                                        error!("Error marking tree as failed to backfill: {err}");
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
                    error!("Could not get trees to backfill from db: {err}");
                    self.sleep_and_increase_delay().await;
                }
            }
        }
    }

    async fn force_backfill_missing_trees(
        &mut self,
        missing_trees: Vec<MissingTree>,
        cn: &impl ConnectionTrait,
    ) -> Result<(), IngesterError> {
        let trees = missing_trees
            .into_iter()
            .map(|tree| backfill_items::ActiveModel {
                tree: Set(tree.tree.as_ref().to_vec()),
                seq: Set(0),
                slot: Set(tree.slot as i64),
                force_chk: Set(true),
                backfilled: Set(false),
                failed: Set(false),
                ..Default::default()
            })
            .collect::<Vec<_>>();

        backfill_items::Entity::insert_many(trees).exec(cn).await?;

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
                debug!("Successfully backfilled tree: {tree_string}, attempt {tries}");

                // Delete extra rows and mark as backfilled.
                match self
                    .delete_extra_rows_and_mark_as_backfilled(tree, max_seq)
                    .await
                {
                    Ok(_) => {
                        // Debug.
                        debug!("Successfully deleted rows up to {max_seq}");
                    }
                    Err(err) => {
                        error!("Error deleting rows and marking as backfilled: {err}");
                        if let Err(err) = self.mark_tree_as_failed(tree).await {
                            error!("Error marking tree as failed to backfill: {err}");
                        }
                    }
                }
            }
            None => {
                // Debug.
                error!("Unexpected error, tree was in list, but no rows found for {tree_string}");
                if let Err(err) = self.mark_tree_as_failed(tree).await {
                    error!("Error marking tree as failed to backfill: {err}");
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

    async fn get_missing_trees(
        &self,
        cn: &impl ConnectionTrait,
    ) -> Result<Vec<MissingTree>, IngesterError> {
        let mut all_trees: HashMap<Pubkey, SlotSeq> = self.fetch_trees_by_gpa().await?;
        debug!("Number of Trees on Chain {}", all_trees.len());

        if let Some(only_trees) = &self.config.backfiller_trees {
            let mut trees = HashSet::with_capacity(only_trees.len());
            for tree in only_trees {
                trees.insert(Pubkey::try_from(tree.as_str()).expect("backfiller tree is invalid"));
            }

            all_trees.retain(|key, _value| trees.contains(key));
            info!(
                "Number of Trees to backfill (with only filter): {}",
                all_trees.len()
            );
        }
        let get_locked_or_failed_trees = Statement::from_string(
            DbBackend::Postgres,
            "SELECT DISTINCT tree FROM backfill_items WHERE failed = true\n\
             OR locked = true"
                .to_string(),
        );
        let locked_trees = cn.query_all(get_locked_or_failed_trees).await?;
        for row in locked_trees.into_iter() {
            let tree = UniqueTree::from_query_result(&row, "")?;
            let key = Pubkey::try_from(tree.tree.as_slice()).unwrap();
            all_trees.remove(&key);
        }
        info!(
            "Number of Trees to backfill (with failed/locked filter): {}",
            all_trees.len()
        );

        // Get all the local trees already in cl_items and remove them
        let get_all_local_trees = Statement::from_string(
            DbBackend::Postgres,
            "SELECT DISTINCT cl_items.tree FROM cl_items".to_string(),
        );
        let force_chk_trees = cn.query_all(get_all_local_trees).await?;
        for row in force_chk_trees.into_iter() {
            let tree = UniqueTree::from_query_result(&row, "")?;
            let key = Pubkey::try_from(tree.tree.as_slice()).unwrap();
            all_trees.remove(&key);
        }
        info!(
            "Number of Trees to backfill (with cl_items existed filter): {}",
            all_trees.len()
        );

        // After removing all the tres in backfill_itemsa nd the trees already in CL Items then return the list
        // of missing trees
        let missing_trees = all_trees
            .into_iter()
            .map(|(k, s)| MissingTree { tree: k, slot: s.0 })
            .collect::<Vec<MissingTree>>();
        if !missing_trees.is_empty() {
            info!("Number of Missing local trees: {}", missing_trees.len());
        } else {
            debug!("No missing trees");
        }
        Ok(missing_trees)
    }

    async fn get_trees_to_backfill(&self) -> Result<Vec<BackfillTree>, DbErr> {
        // Start a db transaction.
        let txn = self.db.begin().await?;

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

        debug!(
            "Number of force check trees to backfill: {} {}",
            force_chk_trees.len(),
            Utc::now()
        );

        for tree in force_chk_trees.iter() {
            let stmt = backfill_items::Entity::update_many()
                .col_expr(backfill_items::Column::Locked, Expr::value(true))
                .filter(backfill_items::Column::Tree.eq(&*tree.tree))
                .build(DbBackend::Postgres);

            if let Err(err) = txn.execute(stmt).await {
                error!(
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

        debug!(
            "Number of multi-row trees to backfill {}",
            multi_row_trees.len()
        );

        for tree in multi_row_trees.iter() {
            let stmt = backfill_items::Entity::update_many()
                .col_expr(backfill_items::Column::Locked, Expr::value(true))
                .filter(backfill_items::Column::Tree.eq(&*tree.tree))
                .build(DbBackend::Postgres);

            if let Err(err) = txn.execute(stmt).await {
                error!(
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
        let address = match Pubkey::try_from(btree.unique_tree.tree.as_slice()) {
            Ok(pubkey) => pubkey,
            Err(error) => {
                return Err(IngesterError::DeserializationError(format!(
                    "failed to parse pubkey: {error:?}"
                )))
            }
        };

        let slots = self.find_slots_via_address(&address).await?;
        let address = btree.unique_tree.tree.clone();
        for slot in slots {
            let gap = GapInfo {
                prev: SimpleBackfillItem {
                    seq: 0,
                    slot: slot as i64,
                },
                curr: SimpleBackfillItem {
                    seq: 0,
                    slot: slot as i64,
                },
            };
            self.plug_gap(&gap, &address).await?;
        }
        Ok(Some(0))
    }

    async fn find_slots_via_address(&self, address: &Pubkey) -> Result<Vec<Slot>, IngesterError> {
        let mut last_sig = None;
        let mut slots = HashSet::new();
        // TODO: Any log running function like this should actually be run in a way that supports re-entry,
        // usually we woudl break the tasks into smaller parralel tasks and we woudl not worry about it, but in this we have several linearally dpendent async tasks
        // and if they fail, it causes a chain reaction of failures since the dependant nature of it affects the next task. Right now you are just naivley looping and
        // hoping for the best what needs to happen is to start saving the state opf each task with the last signature that was retuned iun durable storage.
        // Then if the task fails, you can restart it from the last signature that was returned.
        loop {
            let before = last_sig;
            let sigs = self
                .rpc_client
                .get_signatures_for_address_with_config(
                    address,
                    GetConfirmedSignaturesForAddress2Config {
                        before,
                        until: None,
                        ..GetConfirmedSignaturesForAddress2Config::default()
                    },
                )
                .await
                .map_err(|e| {
                    IngesterError::RpcGetDataError(format!(
                        "GetSignaturesForAddressWithConfig failed {}",
                        e
                    ))
                })?;
            for sig in sigs.iter() {
                let slot = sig.slot;
                let sig = Signature::from_str(&sig.signature).map_err(|e| {
                    IngesterError::RpcDataUnsupportedFormat(format!(
                        "Failed to parse signature {}",
                        e
                    ))
                })?;

                slots.insert(slot);
                last_sig = Some(sig);
            }
            if sigs.is_empty() || sigs.len() < 1000 {
                break;
            }
        }
        Ok(Vec::from_iter(slots))
    }

    #[allow(dead_code)]
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

    async fn fetch_trees_by_gpa(&self) -> Result<HashMap<Pubkey, SlotSeq>, IngesterError> {
        let config = RpcProgramAccountsConfig {
            filters: Some(vec![RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                0,
                vec![1u8],
            ))]),
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
            let (pubkey, mut account) = r;
            let (header_bytes, rest) = account
                .data
                .split_at_mut(CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1);
            let header: ConcurrentMerkleTreeHeader =
                ConcurrentMerkleTreeHeader::try_from_slice(header_bytes)
                    .map_err(|e| IngesterError::RpcGetDataError(e.to_string()))?;

            let auth = Pubkey::find_program_address(&[pubkey.as_ref()], &mpl_bubblegum::ID).0;

            let merkle_tree_size = merkle_tree_get_size(&header)
                .map_err(|e| IngesterError::RpcGetDataError(e.to_string()))?;
            let (tree_bytes, _canopy_bytes) = rest.split_at_mut(merkle_tree_size);
            let seq_bytes = tree_bytes[0..8].try_into().map_err(|_e| {
                IngesterError::RpcGetDataError("Failed to convert seq bytes to array".to_string())
            })?;
            let seq = u64::from_le_bytes(seq_bytes);
            list.insert(pubkey, SlotSeq(header.get_creation_slot(), seq));

            if header.assert_valid_authority(&auth).is_err() {
                continue;
            }
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
            .filter(
                Condition::all()
                    .add(backfill_items::Column::Tree.eq(tree))
                    .add(backfill_items::Column::Backfilled.eq(true)),
            )
            .order_by_desc(backfill_items::Column::Seq)
            .limit(1)
            .build(DbBackend::Postgres);

        let start_seq_vec = MaxSeqItem::find_by_statement(query).all(&self.db).await?;
        let start_seq = start_seq_vec.last().map(|row| row.seq).unwrap_or_default();

        // Get all rows for the tree that have not yet been backfilled.
        let mut query = backfill_items::Entity::find()
            .select_only()
            .column(backfill_items::Column::Seq)
            .column(backfill_items::Column::Slot)
            .filter(
                Condition::all()
                    .add(backfill_items::Column::Seq.gte(start_seq))
                    .add(backfill_items::Column::Tree.eq(tree)),
            )
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
                error!("{}", message);
                return Err(DbErr::Custom(message));
            } else if curr.seq - prev.seq > 1 {
                gaps.push(GapInfo::new(prev.clone(), curr.clone()));
            }
        }

        // Get the max sequence number if any rows were returned from the query.
        let opt_max_seq = rows.last().map(|row| row.seq);

        Ok((opt_max_seq, gaps))
    }

    async fn plug_gap(&mut self, gap: &GapInfo, tree: &[u8]) -> Result<(), IngesterError> {
        // TODO: This needs to make sure all slots are available otherwise it will partially
        // fail and redo the whole backfill process.  So for now checking the max block before
        // looping as a quick workaround.
        let diff = gap.curr.slot - gap.prev.slot;
        let mut num_iter = (diff + 250_000) / 500_000;
        let mut start_slot = gap.prev.slot;
        let mut end_slot = gap.prev.slot + cmp::min(500_000, diff);
        let get_confirmed_slot_tasks = FuturesUnordered::new();
        if num_iter == 0 {
            num_iter = 1;
        }
        for _ in 0..num_iter {
            get_confirmed_slot_tasks.push(self.rpc_client.get_blocks_with_commitment(
                start_slot as u64,
                Some(end_slot as u64),
                CommitmentConfig {
                    commitment: CommitmentLevel::Confirmed,
                },
            ));
            start_slot = end_slot;
            end_slot = cmp::min(end_slot + 500_000, gap.curr.slot);
        }
        let result_slots = get_confirmed_slot_tasks
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|x| x.ok())
            .flatten();
        for slot in result_slots {
            let key = format!("block{}", slot);
            let mut cached_block = self.cache.get(&key).await;
            if cached_block.is_none() {
                debug!("Fetching block {} from RPC", slot);
                let block = EncodedConfirmedBlock::from(
                    self.rpc_client
                        .get_block_with_config(slot, self.rpc_block_config)
                        .await
                        .map_err(|e| IngesterError::RpcGetDataError(e.to_string()))?,
                );
                let cost = cmp::min(32, block.transactions.len() as i64);
                let write = self
                    .cache
                    .try_insert_with_ttl(
                        key.clone(),
                        block,
                        cost,
                        Duration::from_secs(BLOCK_CACHE_DURATION),
                    )
                    .await?;

                if !write {
                    return Err(IngesterError::CacheStorageWriteError(format!(
                        "Cache Write Failed on {} is missing.",
                        &key
                    )));
                }
                self.cache.wait().await?;
                cached_block = self.cache.get(&key).await;
            }
            if cached_block.is_none() {
                return Err(IngesterError::CacheStorageWriteError(format!(
                    "Cache Procedure Failed {} is missing.",
                    &key
                )));
            }
            let block_ref = cached_block.unwrap();
            let block_data = block_ref.value();

            for tx in block_data.transactions.iter() {
                // See if transaction has an error.
                let meta = if let Some(meta) = &tx.meta {
                    if let Some(_err) = &meta.err {
                        continue;
                    }
                    meta
                } else {
                    error!("Unexpected, EncodedTransactionWithStatusMeta struct has no metadata");
                    continue;
                };
                let decoded_tx = if let Some(decoded_tx) = tx.transaction.decode() {
                    decoded_tx
                } else {
                    error!("Unable to decode transaction");
                    continue;
                };
                let sig = decoded_tx.signatures[0].to_string();
                let msg = decoded_tx.message;
                let atl_keys = msg.address_table_lookups();
                let tree = Pubkey::try_from(tree)
                    .map_err(|e| IngesterError::DeserializationError(e.to_string()))?;
                let account_keys = msg.static_account_keys();
                let account_keys = {
                    let mut account_keys_vec = vec![];
                    for key in account_keys.iter() {
                        account_keys_vec.push(key.to_bytes());
                    }
                    if atl_keys.is_some() {
                        if let OptionSerializer::Some(ad) = &meta.loaded_addresses {
                            for i in &ad.writable {
                                let mut output: [u8; 32] = [0; 32];
                                bs58::decode(i).into(&mut output).map_err(|e| {
                                    IngesterError::DeserializationError(e.to_string())
                                })?;
                                account_keys_vec.push(output);
                            }

                            for i in &ad.readonly {
                                let mut output: [u8; 32] = [0; 32];
                                bs58::decode(i).into(&mut output).map_err(|e| {
                                    IngesterError::DeserializationError(e.to_string())
                                })?;
                                account_keys_vec.push(output);
                            }
                        }
                    }
                    account_keys_vec
                };

                // Filter out transactions that don't have to do with the tree we are interested in or
                // the Bubblegum program.
                let tb = tree.to_bytes();
                let bubblegum = blockbuster::programs::bubblegum::ID.to_bytes();
                if account_keys.iter().all(|pk| *pk != tb && *pk != bubblegum) {
                    continue;
                }

                // Serialize data.
                let builder = FlatBufferBuilder::new();
                debug!("Serializing transaction in backfiller {}", sig);
                let tx_wrap = EncodedConfirmedTransactionWithStatusMeta {
                    transaction: tx.to_owned(),
                    slot,
                    block_time: block_data.block_time,
                };
                let builder = seralize_encoded_transaction_with_status(builder, tx_wrap)?;
                self.messenger
                    .send(TRANSACTION_BACKFILL_STREAM, builder.finished_data())
                    .await?;
            }
            drop(block_ref);
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
        debug!("Count of items before delete: {}", test_items.len());
        // Delete all rows in the `backfill_items` table for a specified tree, except for the row with
        // the caller-specified max seq number.  One row for each tree must remain so that gaps can be
        // detected after subsequent inserts.
        backfill_items::Entity::delete_many()
            .filter(
                Condition::all()
                    .add(backfill_items::Column::Tree.eq(tree))
                    .add(backfill_items::Column::Seq.ne(max_seq)),
            )
            .exec(&self.db)
            .await?;

        // Remove any duplicates that have the caller-specified max seq number.  This happens when
        // a transaction that was already handled is replayed during backfilling.
        let items = backfill_items::Entity::find()
            .filter(
                Condition::all()
                    .add(backfill_items::Column::Tree.eq(tree))
                    .add(backfill_items::Column::Seq.ne(max_seq)),
            )
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
        debug!("Count of items after delete: {}", test_items.len());
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
