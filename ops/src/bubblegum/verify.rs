use anyhow::Result;
use clap::Parser;
use das_bubblegum::{verify_bubblegum, BubblegumContext, VerifyArgs};
use das_core::{connect_db, PoolArgs, Rpc, SolanaRpcArgs};
use tracing::info;

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Verify Bubblegum Args
    #[clap(flatten)]
    pub verify_bubblegum: VerifyArgs,

    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,
}

pub async fn run(config: Args) -> Result<()> {
    let database_pool = connect_db(&config.database).await?;

    let solana_rpc = Rpc::from_config(&config.solana);
    let context = BubblegumContext::new(database_pool, solana_rpc);

    let mut reports = verify_bubblegum(context, config.verify_bubblegum).await?;

    while let Some(report) = reports.recv().await {
        info!(
            "Tree: {}, Total Leaves: {}, Incorrect Proofs: {}, Not Found Proofs: {}, Correct Proofs: {}",
            report.tree_pubkey,
            report.total_leaves,
            report.incorrect_proofs,
            report.not_found_proofs,
            report.correct_proofs
        );
    }

    Ok(())
}
