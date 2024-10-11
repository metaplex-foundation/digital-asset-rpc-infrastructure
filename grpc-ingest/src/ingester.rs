use {
    crate::{
        config::{ConfigIngester, REDIS_STREAM_DATA_KEY},
        postgres::{create_pool as pg_create_pool, report_pgpool},
        prom::redis_xack_inc,
        redis::{IngestStream, RedisStreamMessage},
        util::create_shutdown,
    },
    das_core::{DownloadMetadata, DownloadMetadataInfo, DownloadMetadataNotifier},
    futures::{future::BoxFuture, stream::StreamExt},
    program_transformers::{AccountInfo, ProgramTransformer, TransactionInfo},
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

    let accounts_download_metadata_notifier = download_metadata_notifier_v2(
        connection.clone(),
        download_metadata_stream.name.clone(),
        download_metadata_stream_maxlen,
    )?;
    let snapshots_download_metadata_notifier = download_metadata_notifier_v2(
        connection.clone(),
        download_metadata_stream.name.clone(),
        download_metadata_stream_maxlen,
    )?;
    let transactions_download_metadata_notifier = download_metadata_notifier_v2(
        connection.clone(),
        download_metadata_stream.name.clone(),
        download_metadata_stream_maxlen,
    )?;

    let pt_accounts = Arc::new(ProgramTransformer::new(
        pool.clone(),
        accounts_download_metadata_notifier,
    ));
    let pt_snapshots = Arc::new(ProgramTransformer::new(
        pool.clone(),
        snapshots_download_metadata_notifier,
    ));
    let pt_transactions = Arc::new(ProgramTransformer::new(
        pool.clone(),
        transactions_download_metadata_notifier,
    ));
    let http_client = reqwest::Client::builder()
        .timeout(config.download_metadata.request_timeout)
        .build()?;
    let download_metadata = Arc::new(DownloadMetadata::new(http_client, pool.clone()));

    let download_metadata_stream = IngestStream::build()
        .config(config.download_metadata.stream.clone())
        .connection(connection.clone())
        .handler(move |info| {
            let download_metadata = Arc::clone(&download_metadata);

            Box::pin(async move {
                let info = DownloadMetadataInfo::try_parse_msg(info)?;

                download_metadata
                    .handle_download(&info)
                    .await
                    .map_err(Into::into)
            })
        })
        .start()
        .await?;
    let account_stream = IngestStream::build()
        .config(config.accounts.clone())
        .connection(connection.clone())
        .handler(move |info| {
            let pt_accounts = Arc::clone(&pt_accounts);

            Box::pin(async move {
                let info = AccountInfo::try_parse_msg(info)?;

                pt_accounts
                    .handle_account_update(&info)
                    .await
                    .map_err(Into::into)
            })
        })
        .start()
        .await?;
    let transactions_stream = IngestStream::build()
        .config(config.transactions.clone())
        .connection(connection.clone())
        .handler(move |info| {
            let pt_transactions = Arc::clone(&pt_transactions);

            Box::pin(async move {
                let info = TransactionInfo::try_parse_msg(info)?;

                pt_transactions
                    .handle_transaction(&info)
                    .await
                    .map_err(Into::into)
            })
        })
        .start()
        .await?;
    let snapshot_stream = IngestStream::build()
        .config(config.snapshots.clone())
        .connection(connection.clone())
        .handler(move |info| {
            let pt_snapshots = Arc::clone(&pt_snapshots);

            Box::pin(async move {
                let info = AccountInfo::try_parse_msg(info)?;

                pt_snapshots
                    .handle_account_update(&info)
                    .await
                    .map_err(Into::into)
            })
        })
        .start()
        .await?;

    let mut shutdown = create_shutdown()?;

    let report_pool = pool.clone();
    let report_handle = tokio::spawn(async move {
        let pool = report_pool.clone();
        loop {
            sleep(Duration::from_millis(100)).await;
            report_pgpool(pool.clone());
        }
    });

    if let Some(signal) = shutdown.next().await {
        warn!("{signal} received, waiting for spawned tasks...");
    }

    report_handle.abort();

    account_stream.stop().await?;
    transactions_stream.stop().await?;
    download_metadata_stream.stop().await?;
    snapshot_stream.stop().await?;

    pool.close().await;

    Ok::<(), anyhow::Error>(())
}
