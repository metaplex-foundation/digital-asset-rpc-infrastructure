use crate::purge::{start_mint_purge, start_ta_purge, Args as PurgeArgs};
use anyhow::{Ok, Result};
use clap::{Args, Subcommand};

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    // Purge token accounts
    #[clap(name = "token-accounts")]
    TokenAccount(PurgeArgs),
    // Purge mints
    #[clap(name = "mints")]
    Mint(PurgeArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PurgeCommand {
    #[clap(subcommand)]
    pub action: Commands,
}

pub async fn subcommand(subcommand: PurgeCommand) -> Result<()> {
    match subcommand.action {
        Commands::TokenAccount(args) => start_ta_purge(args).await?,
        Commands::Mint(args) => start_mint_purge(args).await?,
    };

    Ok(())
}
