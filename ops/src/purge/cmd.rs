use crate::purge::{start_mint_purge, start_ta_purge, Args as PurgeArgs};
use anyhow::{Ok, Result};
use clap::{Args, Subcommand};
use das_core::{connect_db, PoolArgs, Rpc, SolanaRpcArgs};

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    // Purge token accounts
    #[clap(name = "tokens")]
    TokenAccount(PurgeArgs),
    // Purge mints
    #[clap(name = "mints")]
    Mint(PurgeArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PurgeCommand {
    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,
    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,
    /// The action to take
    #[clap(subcommand)]
    pub action: Commands,
}

pub async fn subcommand(subcommand: PurgeCommand) -> Result<()> {
    let pg_pool = connect_db(subcommand.database).await?;
    let rpc = Rpc::from_config(subcommand.solana);
    match subcommand.action {
        Commands::TokenAccount(args) => start_ta_purge(args, pg_pool, rpc).await?,
        Commands::Mint(args) => start_mint_purge(args, pg_pool, rpc).await?,
    };

    Ok(())
}
