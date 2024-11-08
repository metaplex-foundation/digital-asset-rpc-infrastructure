use super::{program, single};
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// The 'program' command is used to backfill the index against on-chain accounts owned by a program.
    #[clap(name = "program")]
    Program(program::Args),
    /// The 'single' command is used to backfill the index against a single account.
    #[clap(name = "single")]
    Single(single::Args),
    /// The 'nft' command is used to backfill the index against an NFT mint, token metadata, and token account.
    #[clap(name = "nft")]
    Nft(nft::Args),
}

#[derive(Debug, Clone, Args)]
pub struct AccountCommand {
    #[clap(subcommand)]
    pub action: Commands,
}

pub async fn subcommand(subcommand: AccountCommand) -> Result<()> {
    match subcommand.action {
        Commands::Program(args) => {
            program::run(args).await?;
        }
        Commands::Single(args) => {
            single::run(args).await?;
        }
    }

    Ok(())
}
