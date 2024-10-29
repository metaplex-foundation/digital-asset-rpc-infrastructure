mod backfill;
mod error;
mod tree;

use das_core::{MetadataJsonDownloadWorkerArgs, Rpc};
pub use error::ErrorKind;
mod verify;
pub use verify::ProofReport;

use anyhow::Result;
use backfill::worker::{ProgramTransformerWorkerArgs, SignatureWorkerArgs, TreeWorkerArgs};
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

#[derive(Clone)]
pub struct BubblegumContext {
    pub database_pool: sqlx::PgPool,
    pub solana_rpc: Rpc,
}

impl BubblegumContext {
    pub const fn new(database_pool: sqlx::PgPool, solana_rpc: Rpc) -> Self {
        Self {
            database_pool,
            solana_rpc,
        }
    }
}

#[derive(Debug, Parser, Clone)]
pub struct BackfillArgs {
    /// Number of tree crawler workers
    #[arg(long, env, default_value = "20")]
    pub tree_crawler_count: usize,

    /// The list of trees to crawl. If not specified, all trees will be crawled.
    #[arg(long, env, use_value_delimiter = true)]
    pub only_trees: Option<Vec<String>>,

    #[clap(flatten)]
    pub tree_worker: TreeWorkerArgs,
}

pub async fn start_backfill(context: BubblegumContext, args: BackfillArgs) -> Result<()> {
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
    /// The tree to replay.
    #[arg(long, env)]
    pub tree: String,

    /// The list of sequences to replay. If not specified, all sequences will be replayed.
    #[arg(long, env, use_value_delimiter = true)]
    pub only_sequences: Option<Vec<i64>>,

    #[clap(flatten)]
    pub signature_worker: SignatureWorkerArgs,

    #[clap(flatten)]
    pub program_transformer_worker: ProgramTransformerWorkerArgs,

    #[clap(flatten)]
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,
}

pub async fn start_bubblegum_replay(
    context: BubblegumContext,
    args: BubblegumReplayArgs,
) -> Result<()> {
    let pubkey = Pubkey::from_str(&args.tree)
        .map(|pubkey| pubkey.to_bytes().to_vec())
        .map_err(|e| anyhow::anyhow!("Invalid tree pubkey: {:?}", e))?;

    let database_pool = context.database_pool.clone();
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(database_pool);

    let mut query = cl_audits_v2::Entity::find()
        .filter(cl_audits_v2::Column::Tree.eq(pubkey))
        .order_by_asc(cl_audits_v2::Column::Seq);

    if let Some(sequences) = args.only_sequences {
        query = query.filter(cl_audits_v2::Column::Seq.is_in(sequences));
    }

    let cl_audits = query.all(&conn).await?;

    let metadata_json_download_worker_args = args.metadata_json_download_worker.clone();
    let program_transformer_worker_args = args.program_transformer_worker.clone();
    let signature_worker_args = args.signature_worker.clone();

    let metadata_json_download_db_pool = context.database_pool.clone();
    let program_transformer_context = context.clone();
    let signature_context = context.clone();

    let (metadata_json_download_worker, metadata_json_download_sender) =
        metadata_json_download_worker_args.start(metadata_json_download_db_pool)?;

    let (program_transformer_worker, transaction_info_sender) = program_transformer_worker_args
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
}

#[derive(Debug, Parser, Clone)]
pub struct VerifyArgs {
    /// The list of trees to verify. If not specified, all trees will be crawled.
    #[arg(long, env, use_value_delimiter = true)]
    pub only_trees: Option<Vec<String>>,

    #[arg(long, env, default_value = "20")]
    pub max_concurrency: usize,
}

pub async fn verify_bubblegum(
    context: BubblegumContext,
    args: VerifyArgs,
) -> Result<Vec<verify::ProofReport>> {
    let trees = if let Some(ref only_trees) = args.only_trees {
        TreeResponse::find(&context.solana_rpc, only_trees.clone()).await?
    } else {
        TreeResponse::all(&context.solana_rpc).await?
    };

    let mut reports = Vec::new();

    for tree in trees {
        let report = verify::check(context.clone(), tree, args.max_concurrency).await?;

        reports.push(report);
    }

    Ok(reports)
}
