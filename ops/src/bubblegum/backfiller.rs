use anyhow::Result;
use clap::Parser;
use das_backfill::{start_bubblegum_backfill, BubblegumBackfillArgs, BubblegumBackfillContext};
use das_core::{connect_db, PoolArgs, Rpc, SolanaRpcArgs};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Backfill Bubblegum Args
    #[clap(flatten)]
    pub backfill_bubblegum: BubblegumBackfillArgs,

    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,
}

/// Executes the backfilling operation for the tree crawler.
///
/// This function initializes the necessary components for the backfilling operation,
/// such as database connections and RPC clients, and then delegates the actual
/// backfilling logic to the `das_bubblegum_backfill` crate.
///
/// The function undertakes the following key tasks:
/// - Establishes database connections and initializes RPC clients.
/// - Creates a context for the backfilling operation.
/// - Invokes the `start_bubblegum_backfill` function from the `das_bubblegum_backfill` crate.
///
/// # Arguments
///
/// * `config` - A configuration object that includes settings for the backfilling operation,
///   such as database, RPC, and worker configurations.
///
/// # Returns
///
/// This function returns a `Result` which is `Ok` if the backfilling operation is completed
/// successfully, or an `Err` with a relevant error message if any part of the operation
/// encounters issues.
///
/// # Errors
///
/// Potential errors can arise from database connectivity issues or RPC failures.
pub async fn run(config: Args) -> Result<()> {
    let database_pool = connect_db(&config.database).await?;

    let solana_rpc = Rpc::from_config(&config.solana);
    let context = BubblegumBackfillContext::new(database_pool, solana_rpc);

    start_bubblegum_backfill(context, config.backfill_bubblegum).await
}
