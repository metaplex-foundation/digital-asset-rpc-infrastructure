use super::account_info;
use anyhow::Result;
use clap::Parser;
use das_core::{connect_db, MetricsArgs, PoolArgs, Rpc, SolanaRpcArgs};
use futures::future::{ready, FutureExt};
use futures::{stream::FuturesUnordered, StreamExt};
use log::error;
use program_transformers::{AccountInfo, ProgramTransformer};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task;

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Metrics configuration
    #[clap(flatten)]
    pub metrics: MetricsArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,

    /// The batch size to use when fetching accounts
    #[arg(long, env, default_value = "1000")]
    pub batch_size: usize,

    /// The public key of the program to backfill
    #[clap(value_parser = parse_pubkey)]
    pub program: Pubkey,

    /// The maximum buffer size for accounts
    #[arg(long, env, default_value = "10000")]
    pub max_buffer_size: usize,

    /// The number of worker threads
    #[arg(long, env, default_value = "1000")]
    pub account_worker_count: usize,

    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,
}

fn parse_pubkey(s: &str) -> Result<Pubkey, &'static str> {
    Pubkey::try_from(s).map_err(|_| "Failed to parse public key")
}

pub async fn run(config: Args) -> Result<()> {
    let rpc = Rpc::from_config(&config.solana);
    let pool = connect_db(&config.database).await?;
    let num_workers = config.account_worker_count;

    let (tx, mut rx) = mpsc::channel::<Vec<AccountInfo>>(config.max_buffer_size);

    let mut workers = FuturesUnordered::new();
    let program_transformer = Arc::new(ProgramTransformer::new(
        pool,
        Box::new(|_info| ready(Ok(())).boxed()),
        false,
    ));

    let account_info_worker_manager = tokio::spawn(async move {
        while let Some(account_infos) = rx.recv().await {
            if workers.len() >= num_workers {
                workers.next().await;
            }

            for account_info in account_infos {
                let program_transformer = Arc::clone(&program_transformer);

                let worker = task::spawn(async move {
                    if let Err(e) = program_transformer
                        .handle_account_update(&account_info)
                        .await
                    {
                        error!("Failed to handle account update: {:?}", e);
                    }
                });

                workers.push(worker);
            }
        }

        while (workers.next().await).is_some() {}
    });

    let accounts = rpc.get_program_accounts(&config.program, None).await?;
    let accounts_chunks = accounts.chunks(config.batch_size);

    for batch in accounts_chunks {
        let results = futures::future::try_join_all(
            batch
                .iter()
                .cloned()
                .map(|(pubkey, _account)| account_info::fetch(&rpc, pubkey)),
        )
        .await?;

        tx.send(results).await?;
    }

    account_info_worker_manager.await?;

    Ok(())
}
