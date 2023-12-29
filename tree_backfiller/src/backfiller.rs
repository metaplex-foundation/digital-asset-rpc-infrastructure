use crate::db;
use crate::tree::{TreeGapFill, TreeGapModel};
use crate::{
    metrics::{Metrics, MetricsArgs},
    queue,
    rpc::{Rpc, SolanaRpcArgs},
    tree,
};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use digital_asset_types::dao::cl_audits_v2;
use indicatif::HumanDuration;
use log::{error, info};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, SqlxPostgresConnector};
use solana_sdk::signature::Signature;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::{mpsc, Semaphore};

#[derive(Debug, Parser, Clone, ValueEnum, PartialEq, Eq)]
pub enum CrawlDirection {
    Forward,
    Backward,
}

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Number of tree crawler workers
    #[arg(long, env, default_value = "20")]
    pub tree_crawler_count: usize,

    /// The size of the signature channel. This is the number of signatures that can be queued up.
    #[arg(long, env, default_value = "10000")]
    pub signature_channel_size: usize,

    /// The size of the signature channel. This is the number of signatures that can be queued up.
    #[arg(long, env, default_value = "1000")]
    pub gap_channel_size: usize,

    #[arg(long, env, default_value = "100")]
    pub transaction_worker_count: usize,

    #[arg(long, env, default_value = "25")]
    pub gap_worker_count: usize,

    #[arg(long, env, use_value_delimiter = true)]
    pub only_trees: Option<Vec<String>>,

    #[arg(long, env, default_value = "forward")]
    pub crawl_direction: CrawlDirection,

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
    let pool = db::connect(config.database).await?;

    let solana_rpc = Rpc::from_config(config.solana);
    let transaction_solana_rpc = solana_rpc.clone();
    let gap_solana_rpc = solana_rpc.clone();

    let metrics = Metrics::try_from_config(config.metrics)?;
    let tree_metrics = metrics.clone();
    let transaction_metrics = metrics.clone();
    let gap_metrics = metrics.clone();

    let (sig_sender, mut sig_receiver) = mpsc::channel::<Signature>(config.signature_channel_size);
    let (gap_sender, mut gap_receiver) = mpsc::channel::<TreeGapFill>(config.gap_channel_size);

    let gap_count = Counter::new();
    let gap_worker_gap_count = gap_count.clone();

    let transaction_count = Counter::new();
    let transaction_worker_transaction_count = transaction_count.clone();

    let queue = queue::QueuePool::try_from_config(config.queue).await?;

    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(config.transaction_worker_count));

        while let Some(signature) = sig_receiver.recv().await {
            let solana_rpc = transaction_solana_rpc.clone();
            let metrics = transaction_metrics.clone();
            let queue = queue.clone();
            let semaphore = semaphore.clone();
            let count = transaction_worker_transaction_count.clone();

            count.increment();

            tokio::spawn(async move {
                let _permit = semaphore.acquire().await?;

                let timing = Instant::now();

                if let Err(e) = tree::transaction(&solana_rpc, queue, signature).await {
                    error!("tree transaction: {:?}", e);
                    metrics.increment("transaction.failed");
                } else {
                    metrics.increment("transaction.succeeded");
                }

                metrics.time("transaction.queued", timing.elapsed());

                count.decrement();

                Ok::<(), anyhow::Error>(())
            });
        }

        Ok::<(), anyhow::Error>(())
    });

    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(config.gap_worker_count));

        while let Some(gap) = gap_receiver.recv().await {
            let solana_rpc = gap_solana_rpc.clone();
            let metrics = gap_metrics.clone();
            let sig_sender = sig_sender.clone();
            let semaphore = semaphore.clone();
            let count = gap_worker_gap_count.clone();

            count.increment();

            tokio::spawn(async move {
                let _permit = semaphore.acquire().await?;

                let timing = Instant::now();

                if let Err(e) = gap.crawl(&solana_rpc, sig_sender).await {
                    error!("tree transaction: {:?}", e);
                    metrics.increment("gap.failed");
                } else {
                    metrics.increment("gap.succeeded");
                }

                metrics.time("gap.queued", timing.elapsed());

                count.decrement();

                Ok::<(), anyhow::Error>(())
            });
        }

        Ok::<(), anyhow::Error>(())
    });

    let started = Instant::now();

    let trees = if let Some(only_trees) = config.only_trees {
        tree::TreeResponse::find(&solana_rpc, only_trees).await?
    } else {
        tree::TreeResponse::all(&solana_rpc).await?
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
        let semaphore = semaphore.clone();
        let gap_sender = gap_sender.clone();
        let metrics = tree_metrics.clone();
        let pool = pool.clone();
        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

        let crawl_handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await?;

            let timing = Instant::now();

            let mut gaps = TreeGapModel::find(&conn, tree.pubkey)
                .await?
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?;

            let upper_known_seq = cl_audits_v2::Entity::find()
                .filter(cl_audits_v2::Column::Tree.eq(tree.pubkey.as_ref().to_vec()))
                .order_by_desc(cl_audits_v2::Column::Seq)
                .one(&conn)
                .await?;

            let lower_known_seq = cl_audits_v2::Entity::find()
                .filter(cl_audits_v2::Column::Tree.eq(tree.pubkey.as_ref().to_vec()))
                .order_by_asc(cl_audits_v2::Column::Seq)
                .one(&conn)
                .await?;

            if let Some(upper_seq) = upper_known_seq {
                let signature = Signature::try_from(upper_seq.tx.as_ref())?;
                info!(
                    "tree {} has known highest seq {} filling tree from {}",
                    tree.pubkey, upper_seq.seq, signature
                );
                gaps.push(TreeGapFill::new(tree.pubkey, None, Some(signature)));
            } else if tree.seq > 0 {
                info!(
                    "tree {} has no known highest seq but the actual seq is {} filling whole tree",
                    tree.pubkey, tree.seq
                );
                gaps.push(TreeGapFill::new(tree.pubkey, None, None));
            }

            if let Some(lower_seq) = lower_known_seq {
                let signature = Signature::try_from(lower_seq.tx.as_ref())?;

                info!(
                    "tree {} has known lowest seq {} filling tree starting at {}",
                    tree.pubkey, lower_seq.seq, signature
                );
                gaps.push(TreeGapFill::new(tree.pubkey, Some(signature), None));
            }

            let gap_count = gaps.len();

            for gap in gaps {
                if let Err(e) = gap_sender.send(gap).await {
                    metrics.increment("gap.failed");
                    error!("send gap: {:?}", e);
                }
            }

            info!("crawling tree {} with {} gaps", tree.pubkey, gap_count);

            metrics.increment("tree.succeeded");
            metrics.time("tree.crawled", timing.elapsed());

            Ok::<(), anyhow::Error>(())
        });

        crawl_handles.push(crawl_handle);
    }

    futures::future::try_join_all(crawl_handles).await?;
    info!("crawled all trees");

    gap_count.zero().await;
    info!("all gaps queued");

    transaction_count.zero().await;
    info!("all transactions queued");

    metrics.time("job.completed", started.elapsed());

    info!(
        "crawled {} trees in {}",
        tree_count,
        HumanDuration(started.elapsed())
    );

    Ok(())
}
