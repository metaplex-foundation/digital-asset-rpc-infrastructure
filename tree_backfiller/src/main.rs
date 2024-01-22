use anyhow::Result;
use clap::{Parser, Subcommand};
use das_tree_backfiller::backfiller;

#[derive(Debug, Parser)]
#[clap(author, version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    /// The 'run' command is used to cross-reference the index against on-chain accounts.
    /// It crawls through trees and backfills any missed tree transactions.
    #[command(name = "run")]
    Run(backfiller::Args),
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    env_logger::init();

    match args.command {
        Command::Run(config) => backfiller::run(config).await,
    }
}
