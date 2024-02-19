use crate::worker::{perform_metadata_json_task, FetchMetadataJsonError, MetadataJsonTaskError};
use cadence_macros::statsd_count;
use clap::Parser;
use das_tree_backfiller::{
    db,
    metrics::{setup_metrics, MetricsArgs},
};
use log::{error, info};
use reqwest::ClientBuilder;
use tokio::time::Duration;

#[derive(Parser, Clone, Debug)]
pub struct SingleArgs {
    #[clap(flatten)]
    metrics: MetricsArgs,

    #[clap(flatten)]
    database: db::PoolArgs,

    #[arg(long, default_value = "1000")]
    timeout: u64,

    mint: String, // Accept mint as an argument
}

pub async fn run(args: SingleArgs) -> Result<(), anyhow::Error> {
    let pool = db::connect(args.database).await?;

    setup_metrics(args.metrics)?;

    let asset_data = bs58::decode(args.mint.as_str()).into_vec()?;

    let client = ClientBuilder::new()
        .timeout(Duration::from_millis(args.timeout))
        .build()?;

    if let Err(e) = perform_metadata_json_task(client, pool, asset_data).await {
        error!("Asset {} {}", args.mint, e);

        match e {
            MetadataJsonTaskError::Fetch(FetchMetadataJsonError::Response { status, .. }) => {
                let status = &status.to_string();

                statsd_count!("ingester.bgtask.error", 1, "type" => "DownloadMetadata", "status" => status);
            }
            MetadataJsonTaskError::Fetch(FetchMetadataJsonError::Parse { .. }) => {
                statsd_count!("ingester.bgtask.error", 1, "type" => "DownloadMetadata");
            }
            MetadataJsonTaskError::Fetch(FetchMetadataJsonError::GenericReqwest(_e)) => {
                statsd_count!("ingester.bgtask.error", 1, "type" => "DownloadMetadata");
            }
            _ => {
                statsd_count!("ingester.bgtask.error", 1, "type" => "DownloadMetadata");
            }
        }
    } else {
        statsd_count!("ingester.bgtask.success", 1, "type" => "DownloadMetadata");
    }

    info!("Ingesting stopped");

    Ok(())
}
