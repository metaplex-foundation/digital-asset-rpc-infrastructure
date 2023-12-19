mod backfiller;
mod db;
mod metrics;
mod queue;
mod rpc;
mod tree;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
    /// This is particularly useful for ensuring data consistency and completeness.
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
