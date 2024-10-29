use anyhow::Result;
use clap::Parser;
use das_bubblegum::{start_bubblegum_replay, BubblegumContext, BubblegumReplayArgs};
use das_core::{connect_db, PoolArgs, Rpc, SolanaRpcArgs};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,

    #[clap(flatten)]
    pub replay_bubblegum: BubblegumReplayArgs,
}

pub async fn run(config: Args) -> Result<()> {
    let database_pool = connect_db(&config.database).await?;

    let solana_rpc = Rpc::from_config(&config.solana);
    let context = BubblegumContext::new(database_pool, solana_rpc);

    start_bubblegum_replay(context, config.replay_bubblegum).await
}
