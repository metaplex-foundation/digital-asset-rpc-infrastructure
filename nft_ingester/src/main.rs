mod backfiller;
pub mod config;
mod database;
pub mod error;
pub mod metrics;
mod program_transformers;
mod start;
mod stream;
pub mod tasks;
mod account_updates;
mod transaction_notifications;
use start::start;
use tracing::log::error;

#[tokio::main]
async fn main() {
    let tasks = start().await;
    match tasks {
        Ok(mut tasks) => {
            // Wait for signal to shutdown
            match tokio::signal::ctrl_c().await {
                Ok(()) => {}
                Err(err) => {
                    error!("Unable to listen for shutdown signal: {}", err);
                }
            }
            tasks.shutdown().await;
        }
        Err(err) => {
            error!("Unable to start: {}", err);
        }
    }
}