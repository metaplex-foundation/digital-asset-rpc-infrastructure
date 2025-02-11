use super::{audit, backfiller};
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// The 'backfill' command is used to cross-reference the index against on-chain accounts.
    /// It crawls through trees and backfills any missed tree transactions.
    #[clap(name = "backfill")]
    Backfill(backfiller::Args),
    /// The `audit` commands checks `cl_audits_v2` for any failed transactions and logs them to stdout.
    #[clap(name = "audit")]
    Audit(audit::Args),
}

#[derive(Debug, Clone, Args)]
pub struct BubblegumCommand {
    #[clap(subcommand)]
    pub action: Commands,
}

pub async fn subcommand(subcommand: BubblegumCommand) -> Result<()> {
    match subcommand.action {
        Commands::Backfill(args) => {
            backfiller::run(args).await?;
        }
        Commands::Audit(args) => {
            audit::run(args).await?;
        }
    }

    Ok(())
}
