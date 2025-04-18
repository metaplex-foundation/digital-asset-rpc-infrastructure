use anyhow::Result;
use clap::Parser;
use das_core::{
    connect_db, perform_metadata_json_task, DownloadMetadataInfo, DownloadMetadataJsonRetryConfig,
    MetadataJsonDownloadWorkerArgs, PoolArgs,
};

use digital_asset_types::dao::asset_data;
use indicatif::HumanDuration;
use log::{debug, error};
use reqwest::Client;
use sea_orm::{
    ColumnTrait, EntityTrait, JsonValue, PaginatorTrait, QueryFilter, SqlxPostgresConnector,
};
use std::sync::Arc;
use tokio::{sync::mpsc::unbounded_channel, task::JoinSet, time::Instant};

#[derive(Parser, Clone, Debug)]
pub struct ConfigArgs {
    /// The number of db entries to process in a single batch
    #[arg(long, env, default_value = "10")]
    pub batch_size: u64,
}

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Metadata JSON download worker configuration
    #[clap(flatten)]
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,
    // Configuration arguments
    #[clap(flatten)]
    pub config: ConfigArgs,
    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,
}

#[derive(Debug, Clone)]
pub struct MetadataJsonBackfillerContext {
    pub database_pool: sqlx::PgPool,
    pub batch_size: u64,
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,
}

pub async fn start_backfill(context: MetadataJsonBackfillerContext) -> Result<()> {
    let MetadataJsonBackfillerContext {
        database_pool,
        batch_size,
        metadata_json_download_worker:
            MetadataJsonDownloadWorkerArgs {
                metadata_json_download_worker_count,
                metadata_json_download_worker_request_timeout,
            },
    } = context;

    let worker_count = metadata_json_download_worker_count;

    let db_pool = database_pool.clone();

    let (batch_sender, mut batch_receiver) = unbounded_channel::<Vec<DownloadMetadataInfo>>();

    let control = tokio::spawn(async move {
        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(db_pool);

        let mut paginator = asset_data::Entity::find()
            .filter(asset_data::Column::Metadata.eq(JsonValue::String("processing".to_string())))
            .paginate(&conn, batch_size);

        debug!(
            "download metadata json len: {}",
            paginator.num_items().await.unwrap_or(0)
        );

        while let Ok(Some(dm)) = paginator.fetch_and_next().await {
            let download_metadata_info_vec: Vec<DownloadMetadataInfo> = dm
                .into_iter()
                .map(|asset_data| DownloadMetadataInfo {
                    asset_data_id: asset_data.id,
                    uri: asset_data.metadata_url,
                })
                .collect();

            if batch_sender.send(download_metadata_info_vec).is_err() {
                error!("Failed to send batch to worker");
            }
        }
    });

    let mut tasks = JoinSet::new();

    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(
            metadata_json_download_worker_request_timeout,
        ))
        .build()?;

    while let Some(dm_vec) = batch_receiver.recv().await {
        let pool = database_pool.clone();
        if tasks.len() >= worker_count {
            tasks.join_next().await;
        }

        tasks.spawn(fetch_metadata_and_process(
            client.clone(),
            pool.clone(),
            dm_vec,
        ));
    }

    control.await?;

    while tasks.join_next().await.is_some() {}

    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(database_pool.clone());
    let remaining = asset_data::Entity::find()
        .filter(asset_data::Column::Metadata.eq(JsonValue::String("processing".to_string())))
        .count(&conn)
        .await?;

    debug!("Remaining metadata json: {}", remaining);

    Ok(())
}

async fn fetch_metadata_and_process(
    client: reqwest::Client,
    pool: sqlx::PgPool,
    download_metadata_info: Vec<DownloadMetadataInfo>,
) {
    let download_metadata_info = download_metadata_info.to_vec();
    debug!(
        "Spawning metadata fetch task for {} assets",
        download_metadata_info.len()
    );

    for d in download_metadata_info.iter() {
        let timing = Instant::now();
        let asset_data_id = bs58::encode(d.asset_data_id.clone()).into_string();

        if let Err(e) = perform_metadata_json_task(
            client.clone(),
            pool.clone(),
            d,
            Arc::new(DownloadMetadataJsonRetryConfig::default()),
        )
        .await
        {
            error!("Asset {} failed: {}", asset_data_id, e);
        }

        debug!(
            "Asset {} finished in {}",
            asset_data_id,
            HumanDuration(timing.elapsed())
        );
    }
}

pub async fn run(args: Args) -> Result<()> {
    let database_pool = connect_db(&args.database).await?;

    let context = MetadataJsonBackfillerContext {
        database_pool,
        batch_size: args.config.batch_size,
        metadata_json_download_worker: args.metadata_json_download_worker,
    };

    start_backfill(context).await?;

    Ok(())
}
