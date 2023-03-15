mod account_updates;
mod backfiller;
pub mod config;
mod database;
pub mod error;
pub mod metrics;
mod program_transformers;
mod start;
mod stream;
mod ack;
pub mod tasks;
mod transaction_notifications;
use log::{error, info};
use start::start;
use env_logger;


#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting nft_ingester");
    start().await.unwrap();
}
