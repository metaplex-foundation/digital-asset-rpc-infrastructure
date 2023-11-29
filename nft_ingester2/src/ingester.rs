use {
    crate::{
        config::ConfigIngester,
        postgres::{create_pool as pg_create_pool, metrics_pgpool},
        prom::{
            program_transformer_task_status_inc, program_transformer_tasks_total_set,
            ProgramTransformerTaskStatusKind,
        },
        redis::{metrics_xlen, ProgramTransformerInfo, RedisStream},
        util::create_shutdown,
    },
    futures::future::{pending, BoxFuture, FusedFuture, FutureExt},
    program_transformers::{
        error::ProgramTransformerError, DownloadMetadataInfo, DownloadMetadataNotifier,
        ProgramTransformer,
    },
    std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    tokio::{
        signal::unix::SignalKind,
        task::JoinSet,
        time::{sleep, Duration},
    },
    tracing::warn,
};

pub async fn run(config: ConfigIngester) -> anyhow::Result<()> {
    println!("{:#?}", config);

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
            .map(|config| config.stream.clone())
            .collect::<Vec<_>>();
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
        create_notifier(),
        false,
    ));
    let pt_transactions = Arc::new(ProgramTransformer::new(
        pgpool.clone(),
        create_notifier(),
        config.program_transformer.transactions_cl_audits,
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
    let result = loop {
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
            signal = &mut shutdown => {
                let signal = if signal == SignalKind::interrupt() {
                    "SIGINT"
                } else if signal == SignalKind::terminate() {
                    "SIGTERM"
                } else {
                    "UNKNOWN"
                };
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
    };

    redis_messages.shutdown();
    while let Some(result) = pt_tasks.join_next().await {
        result??;
    }
    if !redis_tasks_fut.is_terminated() {
        redis_tasks_fut.await?;
    }
    pgpool.close().await;

    result
}

fn create_notifier() -> DownloadMetadataNotifier {
    Box::new(
        move |_info: DownloadMetadataInfo| -> BoxFuture<
            'static,
            Result<(), Box<dyn std::error::Error + Send + Sync>>,
        > {
            // TODO
            Box::pin(async move { Ok(()) })
        },
    )
}
