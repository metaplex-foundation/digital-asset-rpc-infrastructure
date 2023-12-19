use crate::db;
use crate::{
    metrics::{Metrics, MetricsArgs},
    queue,
    rpc::{Rpc, SolanaRpcArgs},
    tree,
};

use anyhow::Result;
use clap::Parser;
use digital_asset_types::dao::tree_transactions;
use indicatif::HumanDuration;
use log::{error, info};
use sea_orm::SqlxPostgresConnector;
use sea_orm::{sea_query::OnConflict, EntityTrait};
use solana_sdk::signature::Signature;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::{mpsc, Semaphore};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Number of tree crawler workers
    #[arg(long, env, default_value = "20")]
    pub tree_crawler_count: usize,

    /// The size of the signature channel. This is the number of signatures that can be queued up.
    #[arg(long, env, default_value = "10000")]
    pub signature_channel_size: usize,

    #[arg(long, env, default_value = "100")]
    pub transaction_worker_count: usize,

    #[arg(long, env, use_value_delimiter = true)]
    pub only_trees: Option<Vec<String>>,

    /// Database configuration
    #[clap(flatten)]
    pub database: db::PoolArgs,

    /// Redis configuration
    #[clap(flatten)]
    pub queue: queue::QueueArgs,

    /// Metrics configuration
    #[clap(flatten)]
    pub metrics: MetricsArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,
}

/// A thread-safe counter.
pub struct Counter(Arc<AtomicUsize>);

impl Counter {
    /// Creates a new counter initialized to zero.
    pub fn new() -> Self {
        Self(Arc::new(AtomicUsize::new(0)))
    }

    /// Increments the counter by one.
    pub fn increment(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrements the counter by one.
    pub fn decrement(&self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }

    /// Returns the current value of the counter.
    pub fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    /// Returns a future that resolves when the counter reaches zero.
    /// The future periodically checks the counter value and sleeps for a short duration.
    pub fn zero(&self) -> impl std::future::Future<Output = ()> {
        let counter = self.clone();
        async move {
            while counter.get() > 0 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

impl Clone for Counter {
    /// Returns a clone of the counter.
    /// The returned counter shares the same underlying atomic integer.
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Runs the backfilling process for trees.
///
/// This function initializes the necessary components such as the Solana RPC client,
/// database connection, metrics, and worker queues. It then fetches all trees and
/// starts the crawling process for each tree in parallel, respecting the configured
/// concurrency limits. It also listens for signatures and processes transactions
/// concurrently. After crawling all trees, it completes the transaction handling
/// and logs the total time taken for the job.
///
/// # Arguments
///
/// * `config` - The configuration settings for the backfiller, including RPC URLs,
///              database settings, and worker counts.
///
/// # Returns
///
/// This function returns a `Result` which is `Ok` if the backfilling process completes
/// successfully, or an `Error` if any part of the process fails.
pub async fn run(config: Args) -> Result<()> {
    let solana_rpc = Rpc::from_config(config.solana);
    let transaction_solana_rpc = solana_rpc.clone();

    let pool = db::connect(config.database).await?;
    let transaction_pool = pool.clone();

    let metrics = Metrics::try_from_config(config.metrics)?;
    let tree_metrics = metrics.clone();
    let transaction_metrics = metrics.clone();

    let (sig_sender, mut sig_receiver) =
        mpsc::channel::<tree_transactions::ActiveModel>(config.signature_channel_size);

    let transaction_count = Counter::new();
    let transaction_worker_transaction_count = transaction_count.clone();

    let queue = queue::QueuePool::try_from_config(config.queue).await?;

    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(config.transaction_worker_count));

        while let Some(tree_transaction) = sig_receiver.recv().await {
            let solana_rpc = transaction_solana_rpc.clone();
            let metrics = transaction_metrics.clone();
            let queue = queue.clone();
            let semaphore = semaphore.clone();
            let pool = transaction_pool.clone();
            let count = transaction_worker_transaction_count.clone();

            count.increment();

            tokio::spawn(async move {
                let _permit = semaphore.acquire().await?;

                let timing = Instant::now();
                let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

                let inserted_tree_transaction = tree_transactions::Entity::insert(tree_transaction)
                    .on_conflict(
                        OnConflict::column(tree_transactions::Column::Signature)
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec_with_returning(&conn)
                    .await;

                if let Ok(tree_transaction) = inserted_tree_transaction {
                    let signature = Signature::from_str(&tree_transaction.signature)?;

                    if let Err(e) = tree::transaction(&solana_rpc, queue, signature).await {
                        error!("tree transaction: {:?}", e);
                        metrics.increment("transaction.failed");
                    } else {
                        metrics.increment("transaction.succeeded");
                    }

                    metrics.time("transaction.queued", timing.elapsed());
                }

                count.decrement();

                Ok::<(), anyhow::Error>(())
            });
        }

        Ok::<(), anyhow::Error>(())
    });

    let started = Instant::now();

    let trees = if let Some(only_trees) = config.only_trees {
        tree::find(&solana_rpc, only_trees).await?
    } else {
        tree::all(&solana_rpc).await?
    };
    let tree_count = trees.len();

    info!(
        "fetched {} trees in {}",
        tree_count,
        HumanDuration(started.elapsed())
    );

    let semaphore = Arc::new(Semaphore::new(config.tree_crawler_count));
    let mut crawl_handles = Vec::with_capacity(tree_count);

    for tree in trees {
        let client = solana_rpc.clone();
        let semaphore = semaphore.clone();
        let sig_sender = sig_sender.clone();
        let pool = pool.clone();
        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
        let metrics = tree_metrics.clone();

        let crawl_handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await?;

            let timing = Instant::now();

            if let Err(e) = tree.crawl(&client, sig_sender, conn).await {
                metrics.increment("tree.failed");
                error!("crawling tree: {:?}", e);
            } else {
                metrics.increment("tree.succeeded");
            }

            metrics.time("tree.crawled", timing.elapsed());

            Ok::<(), anyhow::Error>(())
        });

        crawl_handles.push(crawl_handle);
    }

    futures::future::try_join_all(crawl_handles).await?;

    transaction_count.zero().await;

    metrics.time("job.completed", started.elapsed());

    info!(
        "crawled {} trees in {}",
        tree_count,
        HumanDuration(started.elapsed())
    );

    Ok(())
}
