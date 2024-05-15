use super::{
    tree::{TreeGapFill, TreeGapModel, TreeResponse},
    BubblegumOpsErrorKind,
};
use anyhow::Result;
use cadence_macros::{statsd_count, statsd_time};
use clap::Parser;
use das_core::{connect_db, setup_metrics, MetricsArgs, PoolArgs, Rpc, SolanaRpcArgs};
use digital_asset_types::dao::cl_audits_v2;
use futures::future::{ready, FutureExt};
use futures::{stream::FuturesUnordered, StreamExt};
use indicatif::HumanDuration;
use log::{debug, error};
use program_transformers::{ProgramTransformer, TransactionInfo};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, SqlxPostgresConnector};
use solana_program::pubkey::Pubkey;
use solana_sdk::instruction::CompiledInstruction;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
    InnerInstruction, InnerInstructions, UiInstruction,
};
use sqlx::PgPool;
use std::time::Instant;
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Number of tree crawler workers
    #[arg(long, env, default_value = "20")]
    pub tree_crawler_count: usize,

    /// The size of the signature channel.
    #[arg(long, env, default_value = "100000")]
    pub signature_channel_size: usize,

    /// The size of the signature channel.
    #[arg(long, env, default_value = "1000")]
    pub gap_channel_size: usize,

    /// The number of transaction workers.
    #[arg(long, env, default_value = "50")]
    pub transaction_worker_count_per_tree: usize,

    /// The number of gap workers.
    #[arg(long, env, default_value = "25")]
    pub gap_worker_count: usize,

    /// The list of trees to crawl. If not specified, all trees will be crawled.
    #[arg(long, env, use_value_delimiter = true)]
    pub only_trees: Option<Vec<String>>,

    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,

    /// Metrics configuration
    #[clap(flatten)]
    pub metrics: MetricsArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,
}

/// Executes the backfilling operation for the tree crawler.
///
/// This function sets up the essential components required for the backfilling operation,
/// including database connections, RPC clients, and worker managers to handle
/// transactions and gaps. It retrieves the necessary trees for crawling and orchestrates
/// the crawling operation across various workers.
///
/// The function undertakes the following key tasks:
/// - Establishes database connections and initializes RPC clients.
/// - Configures channels for inter-component communication.
/// - Deploys worker managers to handle transactions and gaps.
/// - Retrieves trees from the database and oversees their crawling.
/// - Monitors metrics and logs activities throughout the operation.
///
/// # Arguments
///
/// * `config` - A configuration object that includes settings for the backfilling operation,
///   such as database, RPC, and worker configurations.
///
/// # Returns
///
/// This function returns a `Result` which is `Ok` if the backfilling operation is completed
/// successfully, or an `Err` with a relevant error message if any part of the operation
/// encounters issues.
///
/// # Errors
///
/// Potential errors can arise from database connectivity issues, RPC failures,
/// or complications in spawning and managing worker tasks.
pub async fn run(config: Args) -> Result<()> {
    let pool = connect_db(&config.database).await?;

    let solana_rpc = Rpc::from_config(&config.solana);

    setup_metrics(&config.metrics)?;

    let started = Instant::now();

    let trees = if let Some(ref only_trees) = config.only_trees {
        TreeResponse::find(&solana_rpc, only_trees.clone()).await?
    } else {
        TreeResponse::all(&solana_rpc).await?
    };

    let tree_count = trees.len();

    debug!(
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

        let pool = pool.clone();
        let solana_rpc = solana_rpc.clone();

        let handle = spawn_tree_worker(&config, pool, solana_rpc, tree);

        crawl_handles.push(handle);
    }

    futures::future::try_join_all(crawl_handles).await?;

    statsd_time!("job.completed", started.elapsed());

    debug!(
        "crawled {} trees in {}",
        tree_count,
        HumanDuration(started.elapsed())
    );

    Ok(())
}

fn spawn_tree_worker(
    config: &Args,
    pool: PgPool,
    rpc: Rpc,
    tree: TreeResponse,
) -> JoinHandle<Result<(), anyhow::Error>> {
    let config = config.clone();
    let gap_solana_rpc = rpc.clone();
    let gap_pool = pool.clone();

    tokio::spawn(async move {
        let timing = Instant::now();

        let transaction_worker_count = config.transaction_worker_count_per_tree;

        let (sig_sender, mut sig_receiver) =
            mpsc::channel::<Signature>(config.signature_channel_size);
        let gap_sig_sender = sig_sender.clone();

        let (gap_sender, mut gap_receiver) = mpsc::channel::<TreeGapFill>(config.gap_channel_size);
        let (transaction_sender, mut transaction_receiver) =
            mpsc::channel::<TransactionInfo>(config.signature_channel_size);

        let signature_worker_manager = tokio::spawn(async move {
            let mut handlers = FuturesUnordered::new();

            while let Some(signature) = sig_receiver.recv().await {
                if handlers.len() >= transaction_worker_count {
                    handlers.next().await;
                }

                let solana_rpc = rpc.clone();
                let transaction_sender = transaction_sender.clone();

                let handle = spawn_transaction_worker(solana_rpc, transaction_sender, signature);

                handlers.push(handle);
            }

            futures::future::join_all(handlers).await;

            drop(transaction_sender);
        });

        let gap_worker_count = config.gap_worker_count;

        let gap_worker_manager = tokio::spawn(async move {
            let mut handlers = FuturesUnordered::new();
            let sender = gap_sig_sender.clone();

            while let Some(gap) = gap_receiver.recv().await {
                if handlers.len() >= gap_worker_count {
                    handlers.next().await;
                }

                let client = gap_solana_rpc.clone();
                let sender = sender.clone();

                let handle = spawn_crawl_worker(client, sender, gap);

                handlers.push(handle);
            }

            futures::future::join_all(handlers).await;

            drop(sig_sender);
        });

        let transaction_worker_manager = tokio::spawn(async move {
            let mut transactions = Vec::new();
            let pool = pool.clone();

            let program_transformer =
                ProgramTransformer::new(pool, Box::new(|_info| ready(Ok(())).boxed()), true);

            while let Some(gap) = transaction_receiver.recv().await {
                transactions.push(gap);
            }

            transactions.sort_by(|a, b| b.signature.cmp(&a.signature));

            for transaction in transactions {
                if let Err(e) = program_transformer.handle_transaction(&transaction).await {
                    error!("handle transaction: {:?}", e)
                };
            }
        });

        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(gap_pool);

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

        drop(conn);

        if let Some(upper_seq) = upper_known_seq {
            let signature = Signature::try_from(upper_seq.tx.as_ref())?;

            gaps.push(TreeGapFill::new(tree.pubkey, None, Some(signature)));
        } else if tree.seq > 0 {
            gaps.push(TreeGapFill::new(tree.pubkey, None, None));
        }

        if let Some(lower_seq) = lower_known_seq.filter(|seq| seq.seq > 1) {
            let signature = Signature::try_from(lower_seq.tx.as_ref())?;

            gaps.push(TreeGapFill::new(tree.pubkey, Some(signature), None));
        }

        for gap in gaps {
            if let Err(e) = gap_sender.send(gap).await {
                statsd_count!("gap.failed", 1);
                error!("send gap: {:?}", e);
            }
        }

        drop(gap_sender);
        gap_worker_manager.await?;

        signature_worker_manager.await?;

        transaction_worker_manager.await?;

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

pub struct FetchedEncodedTransactionWithStatusMeta(pub EncodedConfirmedTransactionWithStatusMeta);

impl TryFrom<FetchedEncodedTransactionWithStatusMeta> for TransactionInfo {
    type Error = BubblegumOpsErrorKind;

    fn try_from(
        fetched_transaction: FetchedEncodedTransactionWithStatusMeta,
    ) -> Result<Self, Self::Error> {
        let mut account_keys = Vec::new();
        let encoded_transaction_with_status_meta = fetched_transaction.0;

        let ui_transaction: VersionedTransaction = encoded_transaction_with_status_meta
            .transaction
            .transaction
            .decode()
            .ok_or(BubblegumOpsErrorKind::Generic(
                "unable to decode transaction".to_string(),
            ))?;

        let signature = ui_transaction.signatures[0];

        let msg = ui_transaction.message;

        let meta = encoded_transaction_with_status_meta
            .transaction
            .meta
            .ok_or(BubblegumOpsErrorKind::Generic(
                "unable to get meta from transaction".to_string(),
            ))?;

        for address in msg.static_account_keys().iter().copied() {
            account_keys.push(address);
        }
        let ui_loaded_addresses = meta.loaded_addresses;

        let message_address_table_lookup = msg.address_table_lookups();

        if message_address_table_lookup.is_some() {
            if let OptionSerializer::Some(ui_lookup_table) = ui_loaded_addresses {
                for address in ui_lookup_table.writable {
                    account_keys.push(PubkeyString(address).try_into()?);
                }

                for address in ui_lookup_table.readonly {
                    account_keys.push(PubkeyString(address).try_into()?);
                }
            }
        }

        let mut meta_inner_instructions = Vec::new();

        let compiled_instruction = msg.instructions().to_vec();

        let mut instructions = Vec::new();

        for inner in compiled_instruction {
            instructions.push(InnerInstruction {
                stack_height: Some(0),
                instruction: CompiledInstruction {
                    program_id_index: inner.program_id_index,
                    accounts: inner.accounts,
                    data: inner.data,
                },
            });
        }

        meta_inner_instructions.push(InnerInstructions {
            index: 0,
            instructions,
        });

        if let OptionSerializer::Some(inner_instructions) = meta.inner_instructions {
            for ix in inner_instructions {
                let mut instructions = Vec::new();

                for inner in ix.instructions {
                    if let UiInstruction::Compiled(compiled) = inner {
                        instructions.push(InnerInstruction {
                            stack_height: compiled.stack_height,
                            instruction: CompiledInstruction {
                                program_id_index: compiled.program_id_index,
                                accounts: compiled.accounts,
                                data: bs58::decode(compiled.data)
                                    .into_vec()
                                    .map_err(|e| BubblegumOpsErrorKind::Generic(e.to_string()))?,
                            },
                        });
                    }
                }

                meta_inner_instructions.push(InnerInstructions {
                    index: ix.index,
                    instructions,
                });
            }
        }

        Ok(Self {
            slot: encoded_transaction_with_status_meta.slot,
            account_keys,
            signature,
            message_instructions: msg.instructions().to_vec(),
            meta_inner_instructions,
        })
    }
}

async fn queue_transaction<'a>(
    client: Rpc,
    sender: mpsc::Sender<TransactionInfo>,
    signature: Signature,
) -> Result<(), BubblegumOpsErrorKind> {
    let transaction = client.get_transaction(&signature).await?;

    sender
        .send(FetchedEncodedTransactionWithStatusMeta(transaction).try_into()?)
        .await
        .map_err(|e| BubblegumOpsErrorKind::Generic(e.to_string()))?;

    Ok(())
}

fn spawn_transaction_worker(
    client: Rpc,
    sender: mpsc::Sender<TransactionInfo>,
    signature: Signature,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let timing = Instant::now();

        if let Err(e) = queue_transaction(client, sender, signature).await {
            error!("queue transaction: {:?}", e);

            statsd_count!("transaction.failed", 1);
        } else {
            statsd_count!("transaction.succeeded", 1);
        }

        statsd_time!("transaction.queued", timing.elapsed());
    })
}

pub struct PubkeyString(pub String);

impl TryFrom<PubkeyString> for Pubkey {
    type Error = BubblegumOpsErrorKind;

    fn try_from(value: PubkeyString) -> Result<Self, Self::Error> {
        let decoded_bytes = bs58::decode(value.0)
            .into_vec()
            .map_err(|e| BubblegumOpsErrorKind::Generic(e.to_string()))?;

        Pubkey::try_from(decoded_bytes)
            .map_err(|_| BubblegumOpsErrorKind::Generic("unable to convert pubkey".to_string()))
    }
}
