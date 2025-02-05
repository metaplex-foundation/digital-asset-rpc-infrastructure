use {
    crate::{
        config::{ConfigDownloadMetadataPublish, ConfigIngester, REDIS_STREAM_DATA_KEY},
        postgres::{create_pool as pg_create_pool, report_pgpool},
        prom::{download_metadata_publish_time, redis_xadd_status_inc},
        redis::{AccountHandle, DownloadMetadataJsonHandle, IngestStream, TransactionHandle},
        util::create_shutdown,
    },
    das_core::{
        create_download_metadata_notifier, DownloadMetadata, DownloadMetadataInfo,
        DownloadMetadataJsonRetryConfig,
    },
    futures::stream::StreamExt,
    program_transformers::ProgramTransformer,
    redis::aio::MultiplexedConnection,
    std::sync::Arc,
    tokio::{
        sync::mpsc::{unbounded_channel, UnboundedSender},
        task::{JoinHandle, JoinSet},
        time::{sleep, Duration},
    },
    tracing::warn,
};

pub struct DownloadMetadataPublish {
    handle: JoinHandle<()>,
    sender: Option<UnboundedSender<DownloadMetadataInfo>>,
}

impl DownloadMetadataPublish {
    pub fn new(handle: JoinHandle<()>, sender: UnboundedSender<DownloadMetadataInfo>) -> Self {
        Self {
            handle,
            sender: Some(sender),
        }
    }

    pub fn take_sender(&mut self) -> Option<UnboundedSender<DownloadMetadataInfo>> {
        self.sender.take()
    }

    pub async fn stop(self) -> Result<(), tokio::task::JoinError> {
        self.handle.await
    }
}

#[derive(Default)]
pub struct DownloadMetadataPublishBuilder {
    config: Option<ConfigDownloadMetadataPublish>,
    connection: Option<MultiplexedConnection>,
}

impl DownloadMetadataPublishBuilder {
    pub fn build() -> DownloadMetadataPublishBuilder {
        DownloadMetadataPublishBuilder::default()
    }

    pub fn config(mut self, config: ConfigDownloadMetadataPublish) -> Self {
        self.config = Some(config);
        self
    }

    pub fn connection(mut self, connection: MultiplexedConnection) -> Self {
        self.connection = Some(connection);
        self
    }

    pub fn start(self) -> DownloadMetadataPublish {
        let config = self.config.expect("Config must be set");
        let connection = self.connection.expect("Connection must be set");

        let (sender, mut rx) = unbounded_channel::<DownloadMetadataInfo>();
        let stream = config.stream_name;
        let stream_maxlen = config.stream_maxlen;
        let worker_count = config.max_concurrency;

        let handle = tokio::spawn(async move {
            let mut tasks = JoinSet::new();

            while let Some(download_metadata_info) = rx.recv().await {
                if tasks.len() >= worker_count {
                    tasks.join_next().await;
                }

                let mut connection = connection.clone();
                let stream = stream.clone();
                let start_time = tokio::time::Instant::now();

                tasks.spawn(async move {
                    match serde_json::to_vec(&download_metadata_info) {
                        Ok(info_bytes) => {
                            let xadd = redis::cmd("XADD")
                                .arg(&stream)
                                .arg("MAXLEN")
                                .arg("~")
                                .arg(stream_maxlen)
                                .arg("*")
                                .arg(REDIS_STREAM_DATA_KEY)
                                .arg(info_bytes)
                                .query_async::<_, redis::Value>(&mut connection)
                                .await;

                            let status = xadd.map(|_| ()).map_err(|_| ());

                            redis_xadd_status_inc(&stream, "metadata_json", status, 1);
                            let elapsed_time = start_time.elapsed().as_secs_f64();

                            download_metadata_publish_time(elapsed_time);
                        }
                        Err(_) => {
                            tracing::error!("download_metadata_info failed to bytes")
                        }
                    }
                });
            }

            while tasks.join_next().await.is_some() {}
        });

        DownloadMetadataPublish::new(handle, sender)
    }
}

pub async fn run(config: ConfigIngester) -> anyhow::Result<()> {
    let redis_client = redis::Client::open(config.redis)?;
    let connection = redis_client.get_multiplexed_tokio_connection().await?;
    let pool = pg_create_pool(config.postgres).await?;

    let mut download_metadata_publish = DownloadMetadataPublishBuilder::build()
        .connection(connection.clone())
        .config(config.download_metadata_publish)
        .start();

    let download_metadata_json_sender = download_metadata_publish
        .take_sender()
        .expect("Take ownership of sender");

    let create_download_metadata_sender = download_metadata_json_sender.clone();
    let download_metadata_notifier =
        create_download_metadata_notifier(create_download_metadata_sender).await;

    let program_transformer = Arc::new(ProgramTransformer::new(
        pool.clone(),
        download_metadata_notifier,
    ));
    let http_client = reqwest::Client::builder()
        .timeout(config.download_metadata.request_timeout)
        .build()?;

    let download_metadata = Arc::new(DownloadMetadata::new(http_client, pool.clone()));
    let download_metadatas = IngestStream::build()
        .config(config.download_metadata.stream.clone())
        .connection(connection.clone())
        .handler(DownloadMetadataJsonHandle::new(
            Arc::clone(&download_metadata),
            Arc::new(DownloadMetadataJsonRetryConfig::new(
                config.download_metadata.max_attempts,
                config.download_metadata.retry_max_delay_ms,
                config.download_metadata.retry_min_delay_ms,
            )),
        ))
        .start()
        .await?;

    let accounts = IngestStream::build()
        .config(config.accounts)
        .connection(connection.clone())
        .handler(AccountHandle::new(Arc::clone(&program_transformer)))
        .start()
        .await?;

    let transactions = IngestStream::build()
        .config(config.transactions)
        .connection(connection.clone())
        .handler(TransactionHandle::new(Arc::clone(&program_transformer)))
        .start()
        .await?;

    let snapshots = IngestStream::build()
        .config(config.snapshots)
        .connection(connection.clone())
        .handler(AccountHandle::new(program_transformer))
        .start()
        .await?;

    let mut shutdown = create_shutdown()?;

    let report_pool = pool.clone();
    let report = tokio::spawn(async move {
        let pool = report_pool.clone();
        loop {
            sleep(Duration::from_millis(100)).await;
            report_pgpool(pool.clone());
        }
    });

    if let Some(signal) = shutdown.next().await {
        warn!(
            target: "ingester",
            action = "shutdown_signal_received",
            message = "Shutdown signal received, waiting for spawned tasks to complete",
            signal = ?signal
        );
    }

    futures::future::join_all(vec![
        accounts.stop(),
        transactions.stop(),
        snapshots.stop(),
        download_metadatas.stop(),
    ])
    .await
    .into_iter()
    .collect::<anyhow::Result<()>>()?;

    drop(download_metadata_json_sender);
    download_metadata_publish.stop().await?;

    report.abort();

    pool.close().await;

    Ok::<(), anyhow::Error>(())
}
