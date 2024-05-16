use anyhow::Result;

use super::account_info;
use clap::Parser;
use das_core::{connect_db, MetricsArgs, PoolArgs, Rpc, SolanaRpcArgs};
use futures::future::{ready, FutureExt};
use program_transformers::ProgramTransformer;
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,

    /// Metrics configuration
    #[clap(flatten)]
    pub metrics: MetricsArgs,

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

    let program_transformer =
        ProgramTransformer::new(pool, Box::new(|_info| ready(Ok(())).boxed()), false);

    let account_info = account_info::fetch(&rpc, config.account).await?;

    program_transformer
        .handle_account_update(&account_info)
        .await?;

    Ok(())
}
