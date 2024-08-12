use crate::stream::{Receiver, ReceiverArgs};
use crate::worker::{Worker, WorkerArgs};
use clap::Parser;
use das_core::{connect_db, setup_metrics, MetricsArgs, PoolArgs};
use log::info;
use reqwest::ClientBuilder;
use tokio::time::Duration;

#[derive(Parser, Clone, Debug)]
pub struct IngestArgs {
    #[clap(flatten)]
    receiver: ReceiverArgs,

    #[clap(flatten)]
    metrics: MetricsArgs,

    #[clap(flatten)]
    database: PoolArgs,

    #[arg(long, default_value = "1000")]
    timeout: u64,

    #[clap(flatten)]
    worker: WorkerArgs,
}

pub async fn run(args: IngestArgs) -> Result<(), anyhow::Error> {
    let rx = Receiver::try_from_config(args.receiver.into()).await?;

    let pool = connect_db(&args.database).await?;

    setup_metrics(&args.metrics)?;

    let client = ClientBuilder::new()
        .timeout(Duration::from_millis(args.timeout))
        .build()?;

    let worker = Worker::from(args.worker);

    let (tx, handle) = worker.start(pool.clone(), client.clone());

    while let Ok(messages) = rx.recv().await {
        for message in messages.clone() {
            tx.send(message.data).await?;
        }

        let ids: Vec<String> = messages.into_iter().map(|m| m.id).collect();
        rx.ack(&ids).await?;
    }

    drop(tx);

    handle.await?;

    info!("Ingesting stopped");

    Ok(())
}
