use super::tree::{TreeErrorKind, TreeGapFill, TreeGapModel, TreeResponse};
use anyhow::Result;
use cadence_macros::{statsd_count, statsd_time};
use clap::Parser;
use das_core::{
    connect_db, setup_metrics, MetricsArgs, PoolArgs, QueueArgs, QueuePool, Rpc, SolanaRpcArgs,
};
use digital_asset_types::dao::cl_audits_v2;
use flatbuffers::FlatBufferBuilder;
use futures::{stream::FuturesUnordered, StreamExt};
use indicatif::HumanDuration;
use log::{error, info};
use plerkle_serialization::serializer::seralize_encoded_transaction_with_status;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, SqlxPostgresConnector,
};
use solana_sdk::signature::Signature;
use std::time::Instant;
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Number of tree crawler workers
    #[arg(long, env, default_value = "20")]
    pub tree_crawler_count: usize,

    /// The size of the signature channel.
    #[arg(long, env, default_value = "10000")]
    pub signature_channel_size: usize,

    /// The size of the signature channel.
    #[arg(long, env, default_value = "1000")]
    pub gap_channel_size: usize,

    /// The number of transaction workers.
    #[arg(long, env, default_value = "100")]
    pub transaction_worker_count: usize,

    /// The number of gap workers.
    #[arg(long, env, default_value = "25")]
    pub gap_worker_count: usize,

    /// The list of trees to crawl. If not specified, all trees will be crawled.
    #[arg(long, env, use_value_delimiter = true)]
    pub only_trees: Option<Vec<String>>,

    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,

    /// Redis configuration
    #[clap(flatten)]
    pub queue: QueueArgs,

    /// Metrics configuration
    #[clap(flatten)]
    pub metrics: MetricsArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,
}

/// Runs the backfilling process for the tree crawler.
///
/// This function initializes the necessary components for the backfilling process,
/// including database connections, RPC clients, and worker managers for handling
/// transactions and gaps. It then proceeds to fetch the trees that need to be crawled
/// and manages the crawling process across multiple workers.
///
/// The function handles the following major tasks:
/// - Establishing connections to the database and initializing RPC clients.
/// - Setting up channels for communication between different parts of the system.
/// - Spawning worker managers for processing transactions and gaps.
/// - Fetching trees from the database and managing their crawling process.
/// - Reporting metrics and logging information throughout the process.
///
/// # Arguments
///
/// * `config` - A configuration object containing settings for the backfilling process,
///   including database, RPC, and worker configurations.
///
/// # Returns
///
/// This function returns a `Result` which is `Ok` if the backfilling process completes
/// successfully, or an `Err` with an appropriate error message if any part of the process
/// fails.
///
/// # Errors
///
/// This function can return errors related to database connectivity, RPC failures,
/// or issues with spawning and managing worker tasks.
pub async fn run(config: Args) -> Result<()> {
    let pool = connect_db(config.database).await?;

    let solana_rpc = Rpc::from_config(config.solana);
    let transaction_solana_rpc = solana_rpc.clone();
    let gap_solana_rpc = solana_rpc.clone();

    setup_metrics(config.metrics)?;

    let (sig_sender, mut sig_receiver) = mpsc::channel::<Signature>(config.signature_channel_size);
    let gap_sig_sender = sig_sender.clone();
    let (gap_sender, mut gap_receiver) = mpsc::channel::<TreeGapFill>(config.gap_channel_size);

    let queue = QueuePool::try_from_config(config.queue).await?;

    let transaction_worker_count = config.transaction_worker_count;

    let transaction_worker_manager = tokio::spawn(async move {
        let mut handlers = FuturesUnordered::new();

        while let Some(signature) = sig_receiver.recv().await {
            if handlers.len() >= transaction_worker_count {
                handlers.next().await;
            }

            let solana_rpc = transaction_solana_rpc.clone();
            let queue = queue.clone();

            let handle = spawn_transaction_worker(solana_rpc, queue, signature);

            handlers.push(handle);
        }

        futures::future::join_all(handlers).await;
    });

    let gap_worker_count = config.gap_worker_count;

    let gap_worker_manager = tokio::spawn(async move {
        let mut handlers = FuturesUnordered::new();

        while let Some(gap) = gap_receiver.recv().await {
            if handlers.len() >= gap_worker_count {
                handlers.next().await;
            }

            let client = gap_solana_rpc.clone();
            let sender = gap_sig_sender.clone();

            let handle = spawn_crawl_worker(client, sender, gap);

            handlers.push(handle);
        }

        futures::future::join_all(handlers).await;
    });

    let started = Instant::now();

    let trees = if let Some(only_trees) = config.only_trees {
        TreeResponse::find(&solana_rpc, only_trees).await?
    } else {
        TreeResponse::all(&solana_rpc).await?
    };

    let tree_count = trees.len();

    info!(
        "fetched {} trees in {}",
        tree_count,
        HumanDuration(started.elapsed())
    );

    let tree_crawler_count = config.tree_crawler_count;
    let mut crawl_handles = FuturesUnordered::new();

    for tree in trees {
        if crawl_handles.len() >= tree_crawler_count {
            crawl_handles.next().await;
        }

        let sender = gap_sender.clone();
        let pool = pool.clone();
        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

        let handle = spawn_gap_worker(conn, sender, tree);

        crawl_handles.push(handle);
    }

    futures::future::try_join_all(crawl_handles).await?;
    drop(gap_sender);
    info!("crawled all trees");

    gap_worker_manager.await?;
    drop(sig_sender);
    info!("all gaps processed");

    transaction_worker_manager.await?;
    info!("all transactions queued");

    statsd_time!("job.completed", started.elapsed());

    info!(
        "crawled {} trees in {}",
        tree_count,
        HumanDuration(started.elapsed())
    );

    Ok(())
}

fn spawn_gap_worker(
    conn: DatabaseConnection,
    sender: mpsc::Sender<TreeGapFill>,
    tree: TreeResponse,
) -> JoinHandle<Result<(), anyhow::Error>> {
    tokio::spawn(async move {
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

        if let Some(lower_seq) = lower_known_seq.filter(|seq| seq.seq > 1) {
            let signature = Signature::try_from(lower_seq.tx.as_ref())?;

            info!(
                "tree {} has known lowest seq {} filling tree starting at {}",
                tree.pubkey, lower_seq.seq, signature
            );

            gaps.push(TreeGapFill::new(tree.pubkey, Some(signature), None));
        }

        let gap_count = gaps.len();

        for gap in gaps {
            if let Err(e) = sender.send(gap).await {
                statsd_count!("gap.failed", 1);
                error!("send gap: {:?}", e);
            }
        }

        info!("crawling tree {} with {} gaps", tree.pubkey, gap_count);

        statsd_count!("tree.succeeded", 1);
        statsd_time!("tree.crawled", timing.elapsed());

        Ok::<(), anyhow::Error>(())
    })
}

fn spawn_crawl_worker(
    client: Rpc,
    sender: mpsc::Sender<Signature>,
    gap: TreeGapFill,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let timing = Instant::now();

        if let Err(e) = gap.crawl(client, sender).await {
            error!("tree transaction: {:?}", e);

            statsd_count!("gap.failed", 1);
        } else {
            statsd_count!("gap.succeeded", 1);
        }

        statsd_time!("gap.queued", timing.elapsed());
    })
}

async fn queue_transaction(
    client: Rpc,
    queue: QueuePool,
    signature: Signature,
) -> Result<(), TreeErrorKind> {
    let transaction = client.get_transaction(&signature).await?;

    let message = seralize_encoded_transaction_with_status(FlatBufferBuilder::new(), transaction)?;

    queue
        .push_transaction_backfill(message.finished_data())
        .await?;

    Ok(())
}

fn spawn_transaction_worker(client: Rpc, queue: QueuePool, signature: Signature) -> JoinHandle<()> {
    tokio::spawn(async move {
        let timing = Instant::now();

        if let Err(e) = queue_transaction(client, queue, signature).await {
            error!("queue transaction: {:?}", e);

            statsd_count!("transaction.failed", 1);
        } else {
            statsd_count!("transaction.succeeded", 1);
        }

        statsd_time!("transaction.queued", timing.elapsed());
    })
}
