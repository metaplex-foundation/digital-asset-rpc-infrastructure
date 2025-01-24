use anyhow::{Ok, Result};
use clap::Parser;
use das_core::{
    connect_db, perform_metadata_json_task, DownloadMetadataInfo, DownloadMetadataJsonRetryConfig,
    MetadataJsonDownloadWorkerArgs, PoolArgs,
};

use digital_asset_types::dao::asset_data;
use indicatif::HumanDuration;
use log::{debug, error};
use reqwest::Client;
use sea_orm::{ColumnTrait, EntityTrait, JsonValue, QueryFilter, SqlxPostgresConnector};
use std::sync::Arc;
use tokio::{task::JoinHandle, time::Instant};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    #[clap(flatten)]
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,
    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,
}

pub const DEFAULT_METADATA_JSON_DOWNLOAD_WORKER_COUNT: usize = 25;

#[derive(Debug, Clone)]
pub struct MetadataJsonBackfillerContext {
    pub database_pool: sqlx::PgPool,
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,
}

pub async fn start_backfill(context: MetadataJsonBackfillerContext) -> Result<()> {
    let MetadataJsonBackfillerContext {
        database_pool,
        metadata_json_download_worker:
            MetadataJsonDownloadWorkerArgs {
                metadata_json_download_worker_count,
                metadata_json_download_worker_request_timeout,
            },
    } = context;

    let mut worker_count = if metadata_json_download_worker_count > 0 {
        metadata_json_download_worker_count
    } else {
        DEFAULT_METADATA_JSON_DOWNLOAD_WORKER_COUNT
    };
    let database_pool = database_pool.clone();
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(database_pool.clone());

    let download_metadata_info_vec = asset_data::Entity::find()
        .filter(asset_data::Column::Metadata.eq(JsonValue::String("processing".to_string())))
        .all(&conn)
        .await?
        .iter()
        .map(|d| DownloadMetadataInfo::new(d.id.clone(), d.metadata_url.clone(), d.slot_updated))
        .collect::<Vec<DownloadMetadataInfo>>();

    let metadata_vec_len = download_metadata_info_vec.len();
    debug!(
        "Found {} assets to download",
        download_metadata_info_vec.len()
    );

    if metadata_vec_len == 0 {
        return Ok(());
    }

    if worker_count > metadata_vec_len {
        if metadata_vec_len == 1 {
            worker_count = 1;
        } else {
            // If the number of assets is less than the number of workers, we assume each worker will handle 2 assets
            worker_count = metadata_vec_len / 2;
        }
    }

    let excess_tasks = metadata_vec_len % worker_count;
    let mut current_tasks_per_worker = if excess_tasks > 0 {
        metadata_vec_len / worker_count + 1
    } else {
        metadata_vec_len / worker_count
    };

    let mut handlers: Vec<JoinHandle<()>> = Vec::with_capacity(metadata_json_download_worker_count);

    let mut curr_start = 0;
    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(
            metadata_json_download_worker_request_timeout,
        ))
        .build()?;

    debug!("worker_count: {}", worker_count);
    for _ in 0..worker_count {
        let start = curr_start;

        let end = start + current_tasks_per_worker;

        let handler = spawn_metadata_fetch_task(
            client.clone(),
            database_pool.clone(),
            &download_metadata_info_vec[start..end],
        );

        handlers.push(handler);

        current_tasks_per_worker = current_tasks_per_worker.saturating_sub(1);

        curr_start = end;
    }

    futures::future::join_all(handlers).await;

    Ok(())
}

fn spawn_metadata_fetch_task(
    client: reqwest::Client,
    pool: sqlx::PgPool,
    download_metadata_info: &[DownloadMetadataInfo],
) -> JoinHandle<()> {
    let download_metadata_info = download_metadata_info.to_vec();
    tokio::spawn(async move {
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
    })
}

pub async fn run(config: Args) -> Result<()> {
    let database_pool = connect_db(&config.database).await?;

    let context = MetadataJsonBackfillerContext {
        database_pool,
        metadata_json_download_worker: config.metadata_json_download_worker,
    };

    start_backfill(context).await?;

    Ok(())
}
