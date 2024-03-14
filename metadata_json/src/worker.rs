use {
    backon::{ExponentialBuilder, Retryable},
    cadence_macros::{statsd_count, statsd_time},
    clap::Parser,
    digital_asset_types::dao::asset_data,
    futures::{stream::FuturesUnordered, StreamExt},
    indicatif::HumanDuration,
    log::{debug, error},
    reqwest::{Client, Url},
    sea_orm::{entity::*, prelude::*, EntityTrait, SqlxPostgresConnector},
    tokio::{sync::mpsc, task::JoinHandle, time::Instant},
};

#[derive(Parser, Clone, Debug)]
pub struct WorkerArgs {
    #[arg(long, env, default_value = "1000")]
    queue_size: usize,
    #[arg(long, env, default_value = "100")]
    worker_count: usize,
}

pub struct Worker {
    queue_size: usize,
    worker_count: usize,
}

impl From<WorkerArgs> for Worker {
    fn from(args: WorkerArgs) -> Self {
        Self {
            queue_size: args.queue_size,
            worker_count: args.worker_count,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WorkerError {
    #[error("send error: {0}")]
    Send(#[from] mpsc::error::SendError<asset_data::Model>),
    #[error("join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl Worker {
    pub fn start(
        &self,
        pool: sqlx::PgPool,
        client: Client,
    ) -> (mpsc::Sender<Vec<u8>>, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(self.queue_size);
        let worker_count = self.worker_count;

        let handle = tokio::spawn(async move {
            let mut handlers = FuturesUnordered::new();

            while let Some(asset_data) = rx.recv().await {
                if handlers.len() >= worker_count {
                    handlers.next().await;
                }

                let pool = pool.clone();
                let client = client.clone();

                handlers.push(spawn_task(client, pool, asset_data));
            }

            while handlers.next().await.is_some() {}
        });

        (tx, handle)
    }
}

fn spawn_task(client: Client, pool: sqlx::PgPool, asset_data: Vec<u8>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let timing = Instant::now();

        let asset_data_id = asset_data.clone();
        let asset_data_id = bs58::encode(asset_data_id).into_string();

        if let Err(e) = perform_metadata_json_task(client, pool, asset_data).await {
            error!("Asset {} {}", asset_data_id, e);
        } else {
            debug!("Asset {} success", asset_data_id);
        }

        debug!(
            "Asset {} finished in {}",
            asset_data_id,
            HumanDuration(timing.elapsed())
        );

        statsd_time!("ingester.bgtask.finished", timing.elapsed(), "type" => "DownloadMetadata");
    })
}

#[derive(thiserror::Error, Debug)]
pub enum MetadataJsonTaskError {
    #[error("sea orm: {0}")]
    SeaOrm(#[from] sea_orm::DbErr),
    #[error("metadata json: {0}")]
    Fetch(#[from] FetchMetadataJsonError),
    #[error("asset not found in the db")]
    AssetNotFound,
}

pub async fn perform_metadata_json_task(
    client: Client,
    pool: sqlx::PgPool,
    asset_data: Vec<u8>,
) -> Result<asset_data::Model, MetadataJsonTaskError> {
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

    let asset_data_model = asset_data::Entity::find()
        .filter(asset_data::Column::Id.eq(asset_data))
        .one(&conn)
        .await?
        .ok_or(MetadataJsonTaskError::AssetNotFound)?;

    match fetch_metadata_json(client, &asset_data_model.metadata_url).await {
        Ok(metadata) => {
            let mut active_model: asset_data::ActiveModel = asset_data_model.into();
            active_model.metadata = Set(metadata);
            active_model.reindex = Set(Some(false));

            active_model.update(&conn).await.map_err(Into::into)
        }
        Err(e) => {
            let status = match &e {
                FetchMetadataJsonError::Response { status, .. } => status.to_string(),
                FetchMetadataJsonError::Parse { .. } => "parse".to_string(),
                FetchMetadataJsonError::GenericReqwest(_) => "reqwest".to_string(),
                _ => "unhandled".to_string(),
            };

            statsd_count!("ingester.bgtask.error", 1, "type" => "DownloadMetadata", "status" => &status);

            Err(MetadataJsonTaskError::Fetch(e))
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum FetchMetadataJsonError {
    #[error("reqwest: {0}")]
    GenericReqwest(#[from] reqwest::Error),
    #[error("json parse for url({url}) with {source}")]
    Parse { source: reqwest::Error, url: Url },
    #[error("response {status} for url ({url}) with {source}")]
    Response {
        source: reqwest::Error,
        url: Url,
        status: StatusCode,
    },
    #[error("url parse: {0}")]
    Url(#[from] url::ParseError),
}

#[derive(Debug, derive_more::Display)]
pub enum StatusCode {
    Unknown,
    Code(reqwest::StatusCode),
}

async fn fetch_metadata_json(
    client: Client,
    uri: &str,
) -> Result<serde_json::Value, FetchMetadataJsonError> {
    (|| async {
        let url = Url::parse(uri)?;

        let response = client.get(url.clone()).send().await?;

        match response.error_for_status() {
            Ok(res) => res
                .json::<serde_json::Value>()
                .await
                .map_err(|source| FetchMetadataJsonError::Parse { source, url }),
            Err(source) => {
                let status = source
                    .status()
                    .map(StatusCode::Code)
                    .unwrap_or(StatusCode::Unknown);

                Err(FetchMetadataJsonError::Response {
                    source,
                    url,
                    status,
                })
            }
        }
    })
    .retry(&ExponentialBuilder::default())
    .await
}
