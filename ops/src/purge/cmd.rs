use crate::purge::{
    start_cnft_purge, start_mint_purge, start_ta_purge, Args as PurgeArgs, CnftArgs,
};
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
    /// The 'cnft' command is used to remove from the database assets
    ///  for which the DB contains txs that have a `TransactionError`.
    #[clap(name = "cnft")]
    Cnft(CnftArgs),
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
    let pg_pool = connect_db(&subcommand.database).await?;
    let rpc = Rpc::from_config(&subcommand.solana);
    match subcommand.action {
        Commands::TokenAccount(args) => start_ta_purge(args, pg_pool, rpc).await?,
        Commands::Mint(args) => start_mint_purge(args, pg_pool, rpc).await?,
        Commands::Cnft(args) => start_cnft_purge(args, pg_pool, rpc).await?,
    };

    Ok(())
}
