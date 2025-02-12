use super::backfiller;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// The 'backfill' command is used to cross-reference the index against on-chain accounts.
    /// It fetches all metadata json data marked as 'processing' and downloads the metadata json files.
    #[clap(name = "backfill")]
    Backfill(backfiller::Args),
}

#[derive(Debug, Clone, Args)]
pub struct MetadataJsonCommand {
    #[clap(subcommand)]
    pub action: Commands,
}

pub async fn subcommand(subcommand: MetadataJsonCommand) -> Result<()> {
    match subcommand.action {
        Commands::Backfill(args) => {
            backfiller::run(args).await?;
        }
    }

    Ok(())
}