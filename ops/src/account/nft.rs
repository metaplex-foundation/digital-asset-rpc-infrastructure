use std::sync::Arc;

use anyhow::Result;
use tokio::task::JoinHandle;

use super::account_info;
use log::error;

use clap::Parser;
use das_core::{
    connect_db, create_download_metadata_notifier, DownloadMetadataJsonRetryConfig,
    MetadataJsonDownloadWorkerArgs, PoolArgs, Rpc, SolanaRpcArgs,
};
use mpl_token_metadata::accounts::Metadata;
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

    /// NFT Mint address
    #[clap(value_parser = parse_pubkey)]
    pub mint: Pubkey,
}

fn parse_pubkey(s: &str) -> Result<Pubkey, &'static str> {
    Pubkey::try_from(s).map_err(|_| "Failed to parse public key")
}

pub async fn run(config: Args) -> Result<()> {
    let rpc = Rpc::from_config(&config.solana);
    let pool = connect_db(&config.database).await?;
    let metadata_json_download_db_pool = pool.clone();

    let (metadata_json_download_worker, metadata_json_download_sender) =
        config.metadata_json_download_worker.start(
            metadata_json_download_db_pool,
            Arc::new(DownloadMetadataJsonRetryConfig::default()),
        )?;

    let download_metadata_notifier =
        create_download_metadata_notifier(metadata_json_download_sender.clone()).await;

    let mint = config.mint;

    let metadata = Metadata::find_pda(&mint).0;

    let mut accounts_to_fetch = vec![mint, metadata];

    let token_account = rpc.get_token_largest_account(mint).await;

    if let Ok(token_account) = token_account {
        accounts_to_fetch.push(token_account);
    }

    let program_transformer = Arc::new(ProgramTransformer::new(pool, download_metadata_notifier));
    let mut tasks = Vec::new();

    for account in accounts_to_fetch {
        let program_transformer = Arc::clone(&program_transformer);
        let rpc = rpc.clone();

        let task: JoinHandle<Result<(), anyhow::Error>> = tokio::spawn(async move {
            let account_info = account_info::fetch(&rpc, account).await?;
            if let Err(e) = program_transformer
                .handle_account_update(&account_info)
                .await
            {
                error!("Failed to handle account update: {:?}", e);
            }

            Ok(())
        });

        tasks.push(task);
    }

    futures::future::try_join_all(tasks).await?;

    drop(metadata_json_download_sender);

    drop(program_transformer);

    metadata_json_download_worker.await?;

    Ok(())
}
