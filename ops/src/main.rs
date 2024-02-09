mod bubblegum;

use anyhow::Result;
use bubblegum::{subcommand as bubblegum_subcommand, BubblegumCommand};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[clap(author, version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[clap(name = "bubblegum")]
    Bubblegum(BubblegumCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    env_logger::init();

    match args.command {
        Command::Bubblegum(subcommand) => bubblegum_subcommand(subcommand).await?,
    }

    Ok(())
}
