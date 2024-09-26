mod error;
mod gap;
mod tree;
mod worker;

use das_core::{MetadataJsonDownloadWorkerArgs, Rpc};
pub use error::ErrorKind;

use anyhow::Result;
use clap::Parser;
use digital_asset_types::dao::cl_audits_v2;
use futures::{stream::FuturesUnordered, StreamExt};
use sea_orm::ColumnTrait;
use sea_orm::QueryOrder;
use sea_orm::SqlxPostgresConnector;
use sea_orm::{EntityTrait, QueryFilter};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::str::FromStr;
use tracing::error;
use tree::TreeResponse;
use worker::ProgramTransformerWorkerArgs;
use worker::{SignatureWorkerArgs, TreeWorkerArgs};

#[derive(Clone)]
pub struct BubblegumBackfillContext {
    pub database_pool: sqlx::PgPool,
    pub solana_rpc: Rpc,
}

impl BubblegumBackfillContext {
    pub const fn new(database_pool: sqlx::PgPool, solana_rpc: Rpc) -> Self {
        Self {
            database_pool,
            solana_rpc,
        }
    }
}

#[derive(Debug, Parser, Clone)]
pub struct BubblegumBackfillArgs {
    /// Number of tree crawler workers
    #[arg(long, env, default_value = "20")]
    pub tree_crawler_count: usize,

    /// The list of trees to crawl. If not specified, all trees will be crawled.
    #[arg(long, env, use_value_delimiter = true)]
    pub only_trees: Option<Vec<String>>,

    #[clap(flatten)]
    pub tree_worker: TreeWorkerArgs,
}

pub async fn start_bubblegum_backfill(
    context: BubblegumBackfillContext,
    args: BubblegumBackfillArgs,
) -> Result<()> {
    let trees = if let Some(ref only_trees) = args.only_trees {
        TreeResponse::find(&context.solana_rpc, only_trees.clone()).await?
    } else {
        TreeResponse::all(&context.solana_rpc).await?
    };

    let mut crawl_handles = FuturesUnordered::new();

    for tree in trees {
        if crawl_handles.len() >= args.tree_crawler_count {
            crawl_handles.next().await;
        }
        let context = context.clone();
        let handle = args.tree_worker.start(context, tree);

        crawl_handles.push(handle);
    }

    futures::future::try_join_all(crawl_handles).await?;

    Ok(())
}

#[derive(Debug, Parser, Clone)]
pub struct BubblegumReplayArgs {
    /// Number of tree crawler workers
    #[arg(long, env, default_value = "20")]
    pub tree_crawler_count: usize,

    /// The list of trees to crawl. If not specified, all trees will be crawled.
    #[arg(long, env, use_value_delimiter = true)]
    pub trees: Vec<String>,

    #[clap(flatten)]
    pub signature_worker: SignatureWorkerArgs,

    #[clap(flatten)]
    pub program_transformer_worker: ProgramTransformerWorkerArgs,

    #[clap(flatten)]
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,
}

pub async fn start_bubblegum_replay(
    context: BubblegumBackfillContext,
    args: BubblegumReplayArgs,
) -> Result<()> {
    let pubkeys = args
        .trees
        .iter()
        .map(|tree| Pubkey::from_str(tree).map(|pubkey| pubkey.to_bytes().to_vec()))
        .collect::<Result<Vec<Vec<u8>>, _>>()?;

    let mut crawl_handles = FuturesUnordered::new();

    for pubkey in pubkeys {
        if crawl_handles.len() >= args.tree_crawler_count {
            crawl_handles.next().await;
        }
        let database_pool = context.database_pool.clone();

        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(database_pool);

        let cl_audits = cl_audits_v2::Entity::find()
            .filter(cl_audits_v2::Column::Tree.eq(pubkey))
            .order_by_asc(cl_audits_v2::Column::Seq)
            .all(&conn)
            .await?;

        let context = context.clone();
        let metadata_json_download_worker_args = args.metadata_json_download_worker.clone();
        let program_transformer_worker_args = args.program_transformer_worker.clone();
        let signature_worker_args = args.signature_worker.clone();

        let metadata_json_download_database_pool = context.database_pool.clone();
        let handle: tokio::task::JoinHandle<Result<(), anyhow::Error>> = tokio::spawn(async move {
            let metadata_json_download_db_pool = metadata_json_download_database_pool.clone();
            let program_transformer_context = context.clone();
            let signature_context = context.clone();

            let (metadata_json_download_worker, metadata_json_download_sender) =
                metadata_json_download_worker_args.start(metadata_json_download_db_pool)?;

            let (program_transformer_worker, transaction_info_sender) =
                program_transformer_worker_args
                    .start(program_transformer_context, metadata_json_download_sender)?;

            let (signature_worker, signature_sender) =
                signature_worker_args.start(signature_context, transaction_info_sender)?;

            for audit in cl_audits {
                let signature = Signature::try_from(audit.tx.as_ref())?;
                if let Err(e) = signature_sender.send(signature).await {
                    error!("send signature: {:?}", e);
                }
            }

            drop(signature_sender);

            futures::future::try_join3(
                signature_worker,
                program_transformer_worker,
                metadata_json_download_worker,
            )
            .await?;

            Ok(())
        });

        crawl_handles.push(handle);
    }

    futures::future::try_join_all(crawl_handles).await?;

    Ok(())
}
