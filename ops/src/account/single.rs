use std::sync::Arc;

use anyhow::Result;

use super::account_info;
use clap::Parser;
use das_core::{
    connect_db, create_download_metadata_notifier, DownloadMetadataJsonRetryConfig,
    MetadataJsonDownloadWorker, MetadataJsonDownloadWorkerArgs, PoolArgs, Rpc, SolanaRpcArgs,
};
use program_transformers::ProgramTransformer;
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,

    #[clap(flatten)]
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,
    /// The public key of the account to backfill
    #[clap(value_parser = parse_pubkey)]
    pub account: Pubkey,
}

fn parse_pubkey(s: &str) -> Result<Pubkey, &'static str> {
    Pubkey::try_from(s).map_err(|_| "Failed to parse public key")
}

pub async fn run(config: Args) -> Result<()> {
    let rpc = Rpc::from_config(&config.solana);
    let pool = connect_db(&config.database).await?;

    let (download_metadata_sender, download_metadata_worker) = MetadataJsonDownloadWorker::build()
        .pool(pool.clone())
        .request_timeout(
            config
                .metadata_json_download_worker
                .metadata_json_download_worker_request_timeout,
        )
        .worker_count(
            config
                .metadata_json_download_worker
                .metadata_json_download_worker_count,
        )
        .retry(Arc::new(DownloadMetadataJsonRetryConfig::default()))
        .build()?
        .run();

    let download_metadata_notifier =
        create_download_metadata_notifier(download_metadata_sender).await;

    let program_transformer = ProgramTransformer::new(pool, download_metadata_notifier);

    let account_info = account_info::fetch(&rpc, config.account).await?;

    program_transformer
        .handle_account_update(&account_info)
        .await?;

    download_metadata_worker.stop().await?;

    Ok(())
}
