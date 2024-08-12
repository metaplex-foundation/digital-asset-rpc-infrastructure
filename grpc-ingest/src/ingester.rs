use {
    crate::{
        config::{ConfigIngester, ConfigIngesterDownloadMetadata},
        download_metadata::TASK_TYPE,
        postgres::{create_pool as pg_create_pool, metrics_pgpool},
        prom::{
            download_metadata_inserted_total_inc, program_transformer_task_status_inc,
            program_transformer_tasks_total_set, redis_xadd_status_inc,
            ProgramTransformerTaskStatusKind,
        },
        redis::{
            metrics_xlen, ProgramTransformerInfo, RedisStream, RedisStreamMessageInfo,
            TrackedPipeline,
        },
        util::create_shutdown,
    },
    chrono::Utc,
    crypto::{digest::Digest, sha2::Sha256},
    das_core::{DownloadMetadata, DownloadMetadataInfo, DownloadMetadataNotifier},
    digital_asset_types::dao::{sea_orm_active_enums::TaskStatus, tasks},
    futures::{
        future::{pending, BoxFuture, FusedFuture, FutureExt},
        stream::StreamExt,
    },
    program_transformers::{error::ProgramTransformerError, ProgramTransformer},
    redis::{aio::MultiplexedConnection, streams::StreamMaxlen},
    sea_orm::{
        entity::{ActiveModelTrait, ActiveValue},
        error::{DbErr, RuntimeErr},
        SqlxPostgresConnector,
    },
    sqlx::{Error as SqlxError, PgPool},
    std::{
        borrow::Cow,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    },
    tokio::{
        sync::Mutex,
        task::JoinSet,
        time::{sleep, Duration},
    },
    topograph::{
        executor::{Executor, Nonblock, Tokio},
        prelude::*,
    },
    tracing::{debug, error, warn},
};

enum IngestJob {
    SaveMessage(RedisStreamMessageInfo),
    FlushRedisPipe(Arc<Mutex<TrackedPipeline>>, MultiplexedConnection),
}

fn download_metadata_notifier_v2(
    pipe: Arc<Mutex<TrackedPipeline>>,
    stream: String,
    stream_maxlen: usize,
) -> anyhow::Result<DownloadMetadataNotifier> {
    Ok(
            Box::new(
                move |info: DownloadMetadataInfo| -> BoxFuture<
                    'static,
                    Result<(), Box<dyn std::error::Error + Send + Sync>>,
                > {
                    let pipe = Arc::clone(&pipe);
                    let stream = stream.clone();
                    Box::pin(async move {
                        download_metadata_inserted_total_inc();

                        let mut pipe = pipe.lock().await;
                        let info_bytes = serde_json::to_vec(&info)?;

                        // pipe.xadd_maxlen(
                        //     &stream,
                        //     StreamMaxlen::Approx(stream_maxlen),
                        //     "*",
                        //     info_bytes,
                        // );

                        Ok(())
                    })
                },
            ),
        )
}

pub async fn run_v2(config: ConfigIngester) -> anyhow::Result<()> {
    let redis_client = redis::Client::open(config.redis.url.clone())?;
    let connection = redis_client.get_multiplexed_tokio_connection().await?;
    let pool = pg_create_pool(config.postgres).await?;

    let pipe = Arc::new(Mutex::new(TrackedPipeline::default()));
    let (mut redis_messages, redis_tasks_fut) =
        RedisStream::new(config.redis, connection.clone()).await?;
    tokio::pin!(redis_tasks_fut);

    let stream = config.download_metadata.stream.clone();
    let stream_maxlen = config.download_metadata.stream_maxlen;

    let accounts_download_metadata_notifier =
        download_metadata_notifier_v2(Arc::clone(&pipe), stream.clone(), stream_maxlen)?;
    let transactions_download_metadata_notifier =
        download_metadata_notifier_v2(Arc::clone(&pipe), stream.clone(), stream_maxlen)?;

    let pt_accounts = Arc::new(ProgramTransformer::new(
        pool.clone(),
        accounts_download_metadata_notifier,
    ));
    let pt_transactions = Arc::new(ProgramTransformer::new(
        pool.clone(),
        transactions_download_metadata_notifier,
    ));
    let http_client = reqwest::Client::builder()
        .timeout(config.download_metadata.request_timeout)
        .build()?;
    let download_metadata = Arc::new(DownloadMetadata::new(http_client, pool.clone()));

    let exec =
        Executor::builder(Nonblock(Tokio))
            .num_threads(None)
            .build(move |update, _handle| {
                let pt_accounts = Arc::clone(&pt_accounts);
                let pt_transactions = Arc::clone(&pt_transactions);
                let download_metadata = Arc::clone(&download_metadata);

                async move {
                    match update {
                        IngestJob::FlushRedisPipe(pipe, connection) => {
                            let mut pipe = pipe.lock().await;
                            let mut connection = connection;

                            let flush = pipe.flush(&mut connection).await;

                            let status = flush.as_ref().map(|_| ()).map_err(|_| ());
                            let counts = flush.as_ref().unwrap_or_else(|counts| counts);

                            for (stream, count) in counts.iter() {
                                redis_xadd_status_inc(stream, status, *count);
                            }

                            debug!(message = "Redis pipe flushed", ?status, ?counts);
                        }
                        IngestJob::SaveMessage(msg) => {
                            let result = match &msg.get_data() {
                                ProgramTransformerInfo::Account(account) => {
                                    pt_accounts.handle_account_update(account).await
                                }
                                ProgramTransformerInfo::Transaction(transaction) => {
                                    pt_transactions.handle_transaction(transaction).await
                                }
                                ProgramTransformerInfo::MetadataJson(download_metadata_info) => {
                                    download_metadata
                                        .handle_download(download_metadata_info)
                                        .await
                                        .map_err(|e| {
                                            ProgramTransformerError::AssetIndexError(e.to_string())
                                        })
                                }
                            };
                            match result {
                                Ok(()) => program_transformer_task_status_inc(
                                    ProgramTransformerTaskStatusKind::Success,
                                ),
                                Err(ProgramTransformerError::NotImplemented) => {
                                    program_transformer_task_status_inc(
                                        ProgramTransformerTaskStatusKind::NotImplemented,
                                    );
                                    error!("not implemented")
                                }
                                Err(ProgramTransformerError::DeserializationError(error)) => {
                                    program_transformer_task_status_inc(
                                        ProgramTransformerTaskStatusKind::DeserializationError,
                                    );

                                    error!("failed to deserialize {:?}", error)
                                }
                                Err(ProgramTransformerError::ParsingError(error)) => {
                                    program_transformer_task_status_inc(
                                        ProgramTransformerTaskStatusKind::ParsingError,
                                    );

                                    error!("failed to parse {:?}", error)
                                }
                                Err(ProgramTransformerError::DatabaseError(error)) => {
                                    error!("database error for {:?}", error)
                                }
                                Err(ProgramTransformerError::AssetIndexError(error)) => {
                                    error!("indexing error for {:?}", error)
                                }
                                Err(error) => {
                                    error!("failed to handle {:?}", error)
                                }
                            }

                            if let Err(e) = msg.ack() {
                                error!("Failed to acknowledge message: {:?}", e);
                            } else {
                                debug!("Message acknowledged successfully");
                            }
                        }
                    }
                }
            })?;

    let mut shutdown = create_shutdown()?;

    loop {
        tokio::select! {
            _ = sleep(config.download_metadata.pipeline_max_idle) => {
                exec.push(IngestJob::FlushRedisPipe(Arc::clone(&pipe), connection.clone()));
            }
            Some(msg) = redis_messages.recv() => {
                exec.push(IngestJob::SaveMessage(msg));
            }
            Some(signal) = shutdown.next() => {
                warn!("{signal} received, waiting spawned tasks...");
                exec.push(IngestJob::FlushRedisPipe(Arc::clone(&pipe), connection.clone()));
                break;
            }
            result = &mut redis_tasks_fut => {
                if let Err(error) = result {
                    error!("Error in redis_tasks_fut: {:?}", error);
                }
                break;
            }
        }
    }

    redis_messages.shutdown();

    exec.join_async().await;

    pool.close().await;

    Ok::<(), anyhow::Error>(())
}

#[allow(dead_code)]
pub async fn run(config: ConfigIngester) -> anyhow::Result<()> {
    // connect to Redis
    let client = redis::Client::open(config.redis.url.clone())?;
    let connection = client.get_multiplexed_tokio_connection().await?;

    // check stream length for the metrics in spawned task
    let jh_metrics_xlen = tokio::spawn({
        let connection = connection.clone();
        let streams = config
            .redis
            .streams
            .iter()
            .map(|config| config.stream.to_string())
            .collect::<Vec<String>>();
        async move { metrics_xlen(connection, &streams).await }
    });
    tokio::pin!(jh_metrics_xlen);

    // open connection to postgres
    let pgpool = pg_create_pool(config.postgres).await?;
    tokio::spawn({
        let pgpool = pgpool.clone();
        async move { metrics_pgpool(pgpool).await }
    });

    // create redis stream reader
    let (mut redis_messages, redis_tasks_fut) = RedisStream::new(config.redis, connection).await?;
    tokio::pin!(redis_tasks_fut);

    // program transforms related
    let pt_accounts = Arc::new(ProgramTransformer::new(
        pgpool.clone(),
        create_download_metadata_notifier(pgpool.clone(), config.download_metadata.clone())?,
    ));
    let pt_transactions = Arc::new(ProgramTransformer::new(
        pgpool.clone(),
        create_download_metadata_notifier(pgpool.clone(), config.download_metadata)?,
    ));
    let pt_max_tasks_in_process = config.program_transformer.max_tasks_in_process;
    let mut pt_tasks = JoinSet::new();
    let pt_tasks_len = Arc::new(AtomicUsize::new(0));

    tokio::spawn({
        let pt_tasks_len = Arc::clone(&pt_tasks_len);

        async move {
            loop {
                program_transformer_tasks_total_set(pt_tasks_len.load(Ordering::Relaxed));
                sleep(Duration::from_millis(100)).await;
            }
        }
    });

    // read and process messages in the loop
    let mut shutdown = create_shutdown()?;
    loop {
        pt_tasks_len.store(pt_tasks.len(), Ordering::Relaxed);

        let redis_messages_recv = if pt_tasks.len() == pt_max_tasks_in_process {
            pending().boxed()
        } else {
            redis_messages.recv().boxed()
        };
        let pt_tasks_next = if pt_tasks.is_empty() {
            pending().boxed()
        } else {
            pt_tasks.join_next().boxed()
        };

        let msg = tokio::select! {
            result = &mut jh_metrics_xlen => match result {
                Ok(Ok(_)) => unreachable!(),
                Ok(Err(error)) => break Err(error),
                Err(error) => break Err(error.into()),
            },
            Some(signal) = shutdown.next() => {
                warn!("{signal} received, waiting spawned tasks...");
                break Ok(());
            },
            result = &mut redis_tasks_fut => break result,
            msg = redis_messages_recv => match msg {
                Some(msg) => msg,
                None => break Ok(()),
            },
            result = pt_tasks_next => {
                if let Some(result) = result {
                    result??;
                }
                continue;
            }
        };

        pt_tasks.spawn({
            let pt_accounts = Arc::clone(&pt_accounts);
            let pt_transactions = Arc::clone(&pt_transactions);
            async move {
                let result = match &msg.get_data() {
                    ProgramTransformerInfo::Account(account) => {
                        pt_accounts.handle_account_update(account).await
                    }
                    ProgramTransformerInfo::Transaction(transaction) => {
                        pt_transactions.handle_transaction(transaction).await
                    }
                    ProgramTransformerInfo::MetadataJson(_download_metadata_info) => {
                        todo!()
                    }
                };

                macro_rules! log_or_bail {
                    ($action:path, $msg:expr, $error:ident) => {
                        match msg.get_data() {
                            ProgramTransformerInfo::Account(account) => {
                                $action!("{} account {}: {:?}", $msg, account.pubkey, $error)
                            }
                            ProgramTransformerInfo::Transaction(transaction) => {
                                $action!(
                                    "{} transaction {}: {:?}",
                                    $msg,
                                    transaction.signature,
                                    $error
                                )
                            }
                            ProgramTransformerInfo::MetadataJson(_download_metadata_info) => {
                                todo!()
                            }
                        }
                    };
                }

                match result {
                    Ok(()) => program_transformer_task_status_inc(
                        ProgramTransformerTaskStatusKind::Success,
                    ),
                    Err(ProgramTransformerError::NotImplemented) => {
                        program_transformer_task_status_inc(
                            ProgramTransformerTaskStatusKind::NotImplemented,
                        )
                    }
                    Err(ProgramTransformerError::DeserializationError(error)) => {
                        program_transformer_task_status_inc(
                            ProgramTransformerTaskStatusKind::DeserializationError,
                        );
                        log_or_bail!(warn, "failed to deserialize", error)
                    }
                    Err(ProgramTransformerError::ParsingError(error)) => {
                        program_transformer_task_status_inc(
                            ProgramTransformerTaskStatusKind::ParsingError,
                        );
                        log_or_bail!(warn, "failed to parse", error)
                    }
                    Err(ProgramTransformerError::DatabaseError(error)) => {
                        log_or_bail!(anyhow::bail, "database error for", error)
                    }
                    Err(ProgramTransformerError::AssetIndexError(error)) => {
                        log_or_bail!(anyhow::bail, "indexing error for ", error)
                    }
                    Err(error) => {
                        log_or_bail!(anyhow::bail, "failed to handle", error)
                    }
                }

                msg.ack()
            }
        });
    }?;

    tokio::select! {
        Some(signal) = shutdown.next() => {
            anyhow::bail!("{signal} received, force shutdown...");
        }
        result = async move {
            // shutdown `prefetch` channel (but not Receiver)
            redis_messages.shutdown();
            // wait all `program_transformer` spawned tasks
            while let Some(result) = pt_tasks.join_next().await {
                result??;
            }
            // wait all `ack` spawned tasks
            if !redis_tasks_fut.is_terminated() {
                redis_tasks_fut.await?;
            }
            // shutdown database connection
            pgpool.close().await;
            Ok::<(), anyhow::Error>(())
        } => result,
    }
}

fn create_download_metadata_notifier(
    pgpool: PgPool,
    config: ConfigIngesterDownloadMetadata,
) -> anyhow::Result<DownloadMetadataNotifier> {
    let max_attempts = config.max_attempts.try_into()?;
    Ok(Box::new(move |info: DownloadMetadataInfo| -> BoxFuture<
        'static,
        Result<(), Box<dyn std::error::Error + Send + Sync>>,
    > {
        let pgpool = pgpool.clone();
        Box::pin(async move {
            let data = serde_json::to_value(info)?;

            let mut hasher = Sha256::new();
            hasher.input(TASK_TYPE.as_bytes());
            hasher.input(serde_json::to_vec(&data)?.as_slice());
            let hash = hasher.result_str();

            let model = tasks::ActiveModel {
                id: ActiveValue::Set(hash),
                task_type: ActiveValue::Set(TASK_TYPE.to_owned()),
                data: ActiveValue::Set(data),
                status: ActiveValue::Set(TaskStatus::Pending),
                created_at: ActiveValue::Set(Utc::now().naive_utc()),
                locked_until: ActiveValue::Set(None),
                locked_by: ActiveValue::Set(None),
                max_attempts: ActiveValue::Set(max_attempts),
                attempts: ActiveValue::Set(0),
                duration: ActiveValue::Set(None),
                errors: ActiveValue::Set(None),
            };
            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pgpool);

            match model.insert(&conn).await.map(|_mode| ()) {
                // skip unique_violation error
                Err(DbErr::Query(RuntimeErr::SqlxError(SqlxError::Database(dberr)))) if dberr.code() == Some(Cow::Borrowed("23505")) => {},
                value => value?,
            };
            download_metadata_inserted_total_inc();

            Ok(())
        })
    }))
}
