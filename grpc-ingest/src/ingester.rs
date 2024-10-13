use {
    crate::{
        config::{ConfigIngester, REDIS_STREAM_DATA_KEY},
        postgres::{create_pool as pg_create_pool, report_pgpool},
        prom::redis_xack_inc,
        redis::{AccountHandle, DownloadMetadataJsonHandle, IngestStream, TransactionHandle},
        util::create_shutdown,
    },
    das_core::{DownloadMetadata, DownloadMetadataInfo, DownloadMetadataNotifier},
    futures::{future::BoxFuture, stream::StreamExt},
    program_transformers::ProgramTransformer,
    redis::aio::MultiplexedConnection,
    std::sync::Arc,
    tokio::time::{sleep, Duration},
    tracing::warn,
};

fn download_metadata_notifier_v2(
    connection: MultiplexedConnection,
    stream: String,
    stream_maxlen: usize,
) -> anyhow::Result<DownloadMetadataNotifier> {
    Ok(
        Box::new(
            move |info: DownloadMetadataInfo| -> BoxFuture<
                'static,
                Result<(), Box<dyn std::error::Error + Send + Sync>>,
            > {
                let mut connection = connection.clone();
                let stream = stream.clone();
                Box::pin(async move {

                    let info_bytes = serde_json::to_vec(&info)?;

                    redis::cmd("XADD")
                        .arg(&stream)
                        .arg("MAXLEN")
                        .arg("~")
                        .arg(stream_maxlen)
                        .arg("*")
                        .arg(REDIS_STREAM_DATA_KEY)
                        .arg(info_bytes)
                        .query_async(&mut connection)
                        .await?;

                    redis_xack_inc(&stream, 1);

                    Ok(())
                })
            },
        ),
    )
}

pub async fn run(config: ConfigIngester) -> anyhow::Result<()> {
    let redis_client = redis::Client::open(config.redis)?;
    let connection = redis_client.get_multiplexed_tokio_connection().await?;
    let pool = pg_create_pool(config.postgres).await?;

    let download_metadata_stream = config.download_metadata.stream.clone();
    let download_metadata_stream_maxlen = config.download_metadata.stream_maxlen;

    let download_metadata_notifier = download_metadata_notifier_v2(
        connection.clone(),
        download_metadata_stream.name.clone(),
        download_metadata_stream_maxlen,
    )?;

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
        .handler(DownloadMetadataJsonHandle::new(Arc::clone(
            &download_metadata,
        )))
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
        .handler(AccountHandle::new(Arc::clone(&program_transformer)))
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
        warn!("{signal} received, waiting for spawned tasks...");
    }

    report.abort();

    futures::future::join_all(vec![accounts.stop(), transactions.stop(), snapshots.stop()])
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    download_metadatas.stop().await?;

    pool.close().await;

    Ok::<(), anyhow::Error>(())
}
