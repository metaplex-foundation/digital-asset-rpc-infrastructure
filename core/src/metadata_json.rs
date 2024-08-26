use {
    backon::{ExponentialBuilder, Retryable},
    clap::Parser,
    digital_asset_types::dao::asset_data,
    futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt},
    indicatif::HumanDuration,
    log::{debug, error},
    reqwest::{Client, Url as ReqwestUrl},
    sea_orm::{entity::*, SqlxPostgresConnector},
    serde::{Deserialize, Serialize},
    tokio::{
        sync::mpsc::{error::SendError, unbounded_channel, UnboundedSender},
        task::JoinHandle,
        time::Instant,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadMetadataInfo {
    asset_data_id: Vec<u8>,
    uri: String,
    slot: i64,
}

impl DownloadMetadataInfo {
    pub fn new(asset_data_id: Vec<u8>, uri: String, slot: i64) -> Self {
        Self {
            asset_data_id,
            uri: uri.trim().replace('\0', ""),
            slot,
        }
    }

    pub fn into_inner(self) -> (Vec<u8>, String, i64) {
        (self.asset_data_id, self.uri, self.slot)
    }
}

pub type DownloadMetadataNotifier = Box<
    dyn Fn(
            DownloadMetadataInfo,
        ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>
        + Sync
        + Send,
>;

pub async fn create_download_metadata_notifier(
    download_metadata_json_sender: UnboundedSender<DownloadMetadataInfo>,
) -> DownloadMetadataNotifier {
    Box::new(move |info: DownloadMetadataInfo| -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>
    {
        let task = download_metadata_json_sender.send(info).map_err(Into::into);

        Box::pin(async move { task })
    })
}

#[derive(Parser, Clone, Debug, PartialEq, Eq)]
pub struct MetadataJsonDownloadWorkerArgs {
    /// The number of worker threads
    #[arg(long, env, default_value = "25")]
    metadata_json_download_worker_count: usize,
    /// The request timeout in milliseconds
    #[arg(long, env, default_value = "1000")]
    metadata_json_download_worker_request_timeout: u64,
}

impl MetadataJsonDownloadWorkerArgs {
    pub fn start(
        &self,
        pool: sqlx::PgPool,
    ) -> Result<
        (JoinHandle<()>, UnboundedSender<DownloadMetadataInfo>),
        MetadataJsonDownloadWorkerError,
    > {
        let (sender, mut rx) = unbounded_channel::<DownloadMetadataInfo>();
        let worker_count = self.metadata_json_download_worker_count;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(
                self.metadata_json_download_worker_request_timeout,
            ))
            .build()?;

        let handle = tokio::spawn(async move {
            let mut handlers = FuturesUnordered::new();

            while let Some(download_metadata_info) = rx.recv().await {
                if handlers.len() >= worker_count {
                    handlers.next().await;
                }

                let pool = pool.clone();
                let client = client.clone();

                handlers.push(spawn_task(client, pool, download_metadata_info));
            }

            while handlers.next().await.is_some() {}
        });

        Ok((handle, sender))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum MetadataJsonDownloadWorkerError {
    #[error("send error: {0}")]
    Send(#[from] SendError<asset_data::Model>),
    #[error("join error: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
}

fn spawn_task(
    client: Client,
    pool: sqlx::PgPool,
    download_metadata_info: DownloadMetadataInfo,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let timing = Instant::now();
        let asset_data_id =
            bs58::encode(download_metadata_info.asset_data_id.clone()).into_string();

        if let Err(e) = perform_metadata_json_task(client, pool, &download_metadata_info).await {
            error!("Asset {} failed: {}", asset_data_id, e);
        }

        debug!(
            "Asset {} finished in {}",
            asset_data_id,
            HumanDuration(timing.elapsed())
        );
    })
}

#[derive(thiserror::Error, Debug)]
pub enum FetchMetadataJsonError {
    #[error("reqwest: {0}")]
    GenericReqwest(#[from] reqwest::Error),
    #[error("json parse for url({url}) with {source}")]
    Parse {
        source: reqwest::Error,
        url: ReqwestUrl,
    },
    #[error("response {status} for url ({url}) with {source}")]
    Response {
        source: reqwest::Error,
        url: ReqwestUrl,
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
    metadata_json_url: &str,
) -> Result<serde_json::Value, FetchMetadataJsonError> {
    (|| async {
        let url = ReqwestUrl::parse(metadata_json_url)?;

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
    download_metadata_info: &DownloadMetadataInfo,
) -> Result<asset_data::Model, MetadataJsonTaskError> {
    match fetch_metadata_json(client, &download_metadata_info.uri).await {
        Ok(metadata) => {
            let active_model = asset_data::ActiveModel {
                id: Set(download_metadata_info.asset_data_id.clone()),
                metadata: Set(metadata),
                reindex: Set(Some(false)),
                ..Default::default()
            };

            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

            let model = active_model.update(&conn).await?;

            Ok(model)
        }
        Err(e) => Err(MetadataJsonTaskError::Fetch(e)),
    }
}

pub struct DownloadMetadata {
    client: Client,
    pool: sqlx::PgPool,
}

impl DownloadMetadata {
    pub const fn new(client: Client, pool: sqlx::PgPool) -> Self {
        Self { client, pool }
    }

    pub async fn handle_download(
        &self,
        download_metadata_info: &DownloadMetadataInfo,
    ) -> Result<(), MetadataJsonTaskError> {
        perform_metadata_json_task(
            self.client.clone(),
            self.pool.clone(),
            download_metadata_info,
        )
        .await
        .map(|_| ())
    }
}
