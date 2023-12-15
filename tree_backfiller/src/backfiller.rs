use crate::db;
use crate::{
    metrics::{Metrics, MetricsArgs},
    queue, tree,
};

use anyhow::Result;
use clap::Parser;
use indicatif::HumanDuration;
use log::{error, info};
use sea_orm::SqlxPostgresConnector;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::signature::Signature;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Semaphore};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Solana RPC URL
    #[arg(long, env)]
    pub solana_rpc_url: String,

    /// Number of tree crawler workers
    #[arg(long, env, default_value = "100")]
    pub tree_crawler_count: usize,

    /// The size of the signature channel. This is the number of signatures that can be queued up. If the channel is full, the crawler will block until there is space in the channel.
    #[arg(long, env, default_value = "1000")]
    pub signature_channel_size: usize,

    #[arg(long, env, default_value = "1000")]
    pub queue_channel_size: usize,

    #[arg(long, env)]
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
}

/// The main function for running the backfiller.
///
/// This function does the following:
/// 1. Sets up the Solana RPC client and the database connection pool.
/// 2. Initializes the metrics for trees, signatures, and the queue.
/// 3. Creates channels for the queue and signatures.
/// 4. Spawns a new task to handle transactions.
/// 5. Spawns a new task to handle the queue.
/// 6. Fetches all trees and spawns a new task for each tree to crawl it.
/// 7. Waits for all crawling tasks to complete.
/// 8. Waits for the transaction worker count to reach zero.
/// 9. Waits for the queue handler to finish.
/// 10. Logs the total time taken and the number of trees crawled.
///
/// # Arguments
///
/// * `config` - The configuration arguments for the backfiller.
///
/// # Returns
///
/// * `Result<()>` - Returns `Ok(())` if the function runs successfully. Returns an error otherwise.
pub async fn run(config: Args) -> Result<()> {
    let solana_rpc = Arc::new(RpcClient::new(config.solana_rpc_url));
    let sig_solana_rpc = Arc::clone(&solana_rpc);

    let pool = db::connect(config.database).await?;

    let metrics = Metrics::try_from_config(config.metrics)?;
    let tree_metrics = metrics.clone();
    let signature_metrics = metrics.clone();
    let queue_metrics = metrics.clone();

    let (queue_sender, mut queue_receiver) = mpsc::channel::<Vec<u8>>(config.queue_channel_size);
    let signature_queue_sender = queue_sender.clone();
    let (sig_sender, mut sig_receiver) = mpsc::channel::<Signature>(config.signature_channel_size);

    let mut queue = queue::Queue::setup(config.queue).await?;

    let queue_handle = tokio::spawn(async move {
        while let Some(data) = queue_receiver.recv().await {
            if let Err(e) = queue.push(&data).await {
                queue_metrics.increment("transaction.failed");
                error!("pushing to queue: {:?}", e);
            } else {
                queue_metrics.increment("transaction.succeeded");
            }
        }
    });

    let signature_handle = tokio::spawn(async move {
        while let Some(signature) = sig_receiver.recv().await {
            let solana_rpc = Arc::clone(&sig_solana_rpc);
            let queue_sender = signature_queue_sender.clone();
            let metrics = signature_metrics.clone();

            tokio::spawn(async move {
                let timing = Instant::now();

                if let Err(e) = tree::transaction(solana_rpc, queue_sender, signature).await {
                    metrics.increment("transaction.failed");
                    error!("sending to queue: {:?}", e);
                } else {
                    metrics.time("transaction.queued", timing.elapsed());
                }
            });
        }
    });

    let started = Instant::now();

    let trees = tree::all(&solana_rpc).await?;
    let tree_count = trees.len();

    info!(
        "fetched {} trees in {}",
        tree_count,
        HumanDuration(started.elapsed())
    );

    let semaphore = Arc::new(Semaphore::new(config.tree_crawler_count));
    let mut crawl_handles = Vec::with_capacity(tree_count);

    for tree in trees {
        let client = Arc::clone(&solana_rpc);
        let semaphore = Arc::clone(&semaphore);
        let sig_sender = sig_sender.clone();
        let pool = pool.clone();
        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
        let metrics = tree_metrics.clone();

        let crawl_handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await?;

            let timing = Instant::now();

            if let Err(e) = tree.crawl(client, sig_sender, conn).await {
                metrics.increment("tree.failed");
                error!("crawling tree: {:?}", e);
            } else {
                metrics.increment("tree.completed");
            }

            metrics.time("tree.crawled", timing.elapsed());

            Ok::<(), anyhow::Error>(())
        });

        crawl_handles.push(crawl_handle);
    }

    futures::future::try_join_all(crawl_handles).await?;
    drop(sig_sender);

    signature_handle.await?;
    drop(queue_sender);

    queue_handle.await?;

    metrics.time("job.completed", started.elapsed());

    info!(
        "crawled {} trees in {}",
        tree_count,
        HumanDuration(started.elapsed())
    );

    Ok(())
}
