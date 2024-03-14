use crate::worker::perform_metadata_json_task;
use clap::Parser;
use das_core::{connect_db, setup_metrics, MetricsArgs, PoolArgs};
use log::{debug, error};
use reqwest::ClientBuilder;
use tokio::time::Duration;

#[derive(Parser, Clone, Debug)]
pub struct SingleArgs {
    #[clap(flatten)]
    metrics: MetricsArgs,

    #[clap(flatten)]
    database: PoolArgs,

    #[arg(long, default_value = "1000")]
    timeout: u64,

    mint: String, // Accept mint as an argument
}

pub async fn run(args: SingleArgs) -> Result<(), anyhow::Error> {
    let pool = connect_db(args.database).await?;

    setup_metrics(args.metrics)?;

    let asset_data = bs58::decode(args.mint.as_str()).into_vec()?;

    let client = ClientBuilder::new()
        .timeout(Duration::from_millis(args.timeout))
        .build()?;

    if let Err(e) = perform_metadata_json_task(client, pool, asset_data).await {
        error!("{}", e);
    } else {
        debug!("Success");
    }

    Ok(())
}
