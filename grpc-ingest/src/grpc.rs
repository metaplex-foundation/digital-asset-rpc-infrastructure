use {
    crate::{
        config::ConfigGrpc,
        prom::redis_xadd_status_inc,
        redis::{metrics_xlen, TrackedPipeline},
        util::create_shutdown,
    },
    anyhow::Context,
    futures::{channel::mpsc, stream::StreamExt, SinkExt},
    lru::LruCache,
    opentelemetry_sdk::trace::Config,
    redis::{
        aio::MultiplexedConnection, streams::StreamMaxlen, Pipeline, RedisResult,
        Value as RedisValue,
    },
    serde::de,
    sqlx::pool,
    std::{collections::HashMap, num::NonZeroUsize, sync::Arc, time::Duration},
    tokio::{
        spawn,
        sync::Mutex,
        task::JoinSet,
        time::{sleep, Instant},
    },
    topograph::{
        executor::{Executor, Nonblock, Tokio},
        prelude::*,
    },
    tracing::{debug, warn},
    tracing_subscriber::field::debug,
    yellowstone_grpc_client::GeyserGrpcClient,
    yellowstone_grpc_proto::{
        geyser::{SubscribeRequest, SubscribeUpdate},
        prelude::subscribe_update::UpdateOneof,
        prost::Message,
    },
    yellowstone_grpc_tools::config::GrpcRequestToProto,
};

enum GrpcJob {
    FlushRedisPipe,
    ProcessSubscribeUpdate(Box<SubscribeUpdate>),
}

pub async fn run_v2(config: ConfigGrpc) -> anyhow::Result<()> {
    let redis_client = redis::Client::open(config.redis.url.clone())?;
    let config = Arc::new(config);
    let connection = redis_client.get_multiplexed_tokio_connection().await?;

    let mut shutdown = create_shutdown()?;

    let pipe = Arc::new(Mutex::new(TrackedPipeline::default()));

    let mut accounts = HashMap::with_capacity(1);
    let mut transactions = HashMap::with_capacity(1);

    accounts.insert("das".to_string(), config.accounts.filter.clone().to_proto());
    transactions.insert(
        "das".to_string(),
        config.transactions.filter.clone().to_proto(),
    );

    let request = SubscribeRequest {
        accounts,
        transactions,
        ..Default::default()
    };

    let mut dragon_mouth_client =
        GeyserGrpcClient::build_from_shared(config.geyser_endpoint.clone())?
            .x_token(config.x_token.clone())?
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(10))
            .connect()
            .await
            .context("failed to connect to gRPC")?;

    let (_subscribe_tx, stream) = dragon_mouth_client
        .subscribe_with_request(Some(request))
        .await?;
    tokio::pin!(stream);

    let pool_config = Arc::clone(&config);
    let pool_connection = connection.clone();
    let pool_pipe = Arc::clone(&pipe);

    let exec =
        Executor::builder(Nonblock(Tokio))
            .num_threads(None)
            .build(move |update, _handle| {
                let config = Arc::clone(&pool_config);
                let connection = pool_connection.clone();
                let pipe = Arc::clone(&pool_pipe);

                async move {
                    match update {
                        GrpcJob::FlushRedisPipe => {
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
                        GrpcJob::ProcessSubscribeUpdate(update) => {
                            let accounts_stream = config.accounts.stream.clone();
                            let accounts_stream_maxlen = config.accounts.stream_maxlen;
                            let accounts_stream_data_key = config.accounts.stream_data_key.clone();
                            let transactions_stream = config.transactions.stream.clone();
                            let transactions_stream_maxlen = config.transactions.stream_maxlen;
                            let transactions_stream_data_key =
                                config.transactions.stream_data_key.clone();

                            let SubscribeUpdate { update_oneof, .. } = *update;

                            let mut pipe = pipe.lock().await;

                            if let Some(update) = update_oneof {
                                match update {
                                    UpdateOneof::Account(account) => {
                                        pipe.xadd_maxlen(
                                            &accounts_stream,
                                            StreamMaxlen::Approx(accounts_stream_maxlen),
                                            "*",
                                            &[(&accounts_stream_data_key, account.encode_to_vec())],
                                        );

                                        debug!(message = "Account update", ?account,);
                                    }
                                    UpdateOneof::Transaction(transaction) => {
                                        pipe.xadd_maxlen(
                                            &transactions_stream,
                                            StreamMaxlen::Approx(transactions_stream_maxlen),
                                            "*",
                                            &[(
                                                &transactions_stream_data_key,
                                                transaction.encode_to_vec(),
                                            )],
                                        );

                                        debug!(message = "Transaction update", ?transaction);
                                    }
                                    var => warn!(message = "Unknown update variant", ?var),
                                }
                            }

                            if pipe.size() >= config.redis.pipeline_max_size {
                                // handle.push(GrpcJob::FlushRedisPipe);
                            }
                        }
                    }
                }
            })?;

    let deadline_config = Arc::clone(&config);

    loop {
        tokio::select! {
            _ = sleep(deadline_config.redis.pipeline_max_idle) => {
                exec.push(GrpcJob::FlushRedisPipe);
            }
            Some(Ok(msg)) = stream.next() => {
                debug!(message = "Received gRPC message", ?msg);
                exec.push(GrpcJob::ProcessSubscribeUpdate(Box::new(msg)));
            }
            _ = shutdown.next() => {
                exec.push(GrpcJob::FlushRedisPipe);
                break;
            }
        }
    }

    exec.join_async().await;

    Ok(())
}

pub async fn run(config: ConfigGrpc) -> anyhow::Result<()> {
    let config = Arc::new(config);
    let (tx, mut rx) = mpsc::channel::<UpdateOneof>(config.geyser_update_message_buffer_size); // Adjust buffer size as needed

    // Connect to Redis
    let client = redis::Client::open(config.redis.url.clone())?;
    let connection = client.get_multiplexed_tokio_connection().await?;

    // Check stream length for the metrics
    let jh_metrics_xlen = spawn({
        let connection = connection.clone();
        let streams = vec![
            config.accounts.stream.clone(),
            config.transactions.stream.clone(),
        ];
        async move { metrics_xlen(connection, &streams).await }
    });
    tokio::pin!(jh_metrics_xlen);

    // Spawn gRPC client connections
    let config = Arc::clone(&config);
    let mut tx = tx.clone();

    let mut client = GeyserGrpcClient::build_from_shared(config.geyser_endpoint.clone())?
        .x_token(config.x_token.clone())?
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(10))
        .connect()
        .await
        .context("failed to connect to gRPC")?;

    let grc_config = Arc::clone(&config);
    spawn(async move {
        let mut accounts = HashMap::with_capacity(1);
        let mut transactions = HashMap::with_capacity(1);

        accounts.insert(
            "das".to_string(),
            grc_config.accounts.filter.clone().to_proto(),
        );
        transactions.insert(
            "das".to_string(),
            grc_config.transactions.filter.clone().to_proto(),
        );

        let request = SubscribeRequest {
            accounts,
            transactions,
            ..Default::default()
        };

        let (_subscribe_tx, mut stream) = client.subscribe_with_request(Some(request)).await?;

        while let Some(Ok(msg)) = stream.next().await {
            if let Some(update) = msg.update_oneof {
                tx.send(update)
                    .await
                    .expect("Failed to send update to management thread");
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    // Management thread
    let mut shutdown = create_shutdown()?;
    let mut tasks = JoinSet::new();
    let mut pipe = redis::pipe();
    let mut pipe_accounts = 0;
    let mut pipe_transactions = 0;

    let pipeline_max_idle = config.redis.pipeline_max_idle;
    let deadline = sleep(pipeline_max_idle);
    tokio::pin!(deadline);

    let result = loop {
        tokio::select! {
            result = &mut jh_metrics_xlen => match result {
                Ok(Ok(_)) => unreachable!(),
                Ok(Err(error)) => break Err(error),
                Err(error) => break Err(error.into()),
            },
            Some(signal) = shutdown.next() => {
                warn!("{signal} received, waiting spawned tasks...");
                break Ok(());
            },
            Some(update) = rx.next() => {
                match update {
                    UpdateOneof::Account(account) => {

                        pipe.xadd_maxlen(
                            &config.accounts.stream,
                            StreamMaxlen::Approx(config.accounts.stream_maxlen),
                            "*",
                            &[(&config.accounts.stream_data_key, account.encode_to_vec())],
                        );

                        pipe_accounts += 1;
                    }
                    UpdateOneof::Transaction(transaction) => {
                        pipe.xadd_maxlen(
                            &config.transactions.stream,
                            StreamMaxlen::Approx(config.transactions.stream_maxlen),
                            "*",
                            &[(&config.transactions.stream_data_key, transaction.encode_to_vec())]
                        );

                        pipe_transactions += 1;
                    }
                    _ => continue,
                }
                if pipe_accounts + pipe_transactions >= config.redis.pipeline_max_size {
                    let mut pipe = std::mem::replace(&mut pipe, redis::pipe());
                    let pipe_accounts = std::mem::replace(&mut pipe_accounts, 0);
                    let pipe_transactions = std::mem::replace(&mut pipe_transactions, 0);
                    deadline.as_mut().reset(Instant::now() + config.redis.pipeline_max_idle);

                    tasks.spawn({
                        let mut connection = connection.clone();
                        let config = Arc::clone(&config);
                        async move {
                            let result: RedisResult<RedisValue> =
                                pipe.atomic().query_async(&mut connection).await;

                            let status = result.map(|_| ()).map_err(|_| ());
                            redis_xadd_status_inc(&config.accounts.stream, status, pipe_accounts);
                            redis_xadd_status_inc(&config.transactions.stream, status, pipe_transactions);

                            Ok::<(), anyhow::Error>(())
                        }
                    });
                }
            },
            _ = &mut deadline => {
                if pipe_accounts + pipe_transactions > 0 {
                    let mut pipe = std::mem::replace(&mut pipe, redis::pipe());
                    let pipe_accounts = std::mem::replace(&mut pipe_accounts, 0);
                    let pipe_transactions = std::mem::replace(&mut pipe_transactions, 0);
                    deadline.as_mut().reset(Instant::now() + config.redis.pipeline_max_idle);

                    tasks.spawn({
                        let mut connection = connection.clone();
                        let config = Arc::clone(&config);
                        async move {
                            let result: RedisResult<RedisValue> =
                                pipe.atomic().query_async(&mut connection).await;

                            let status = result.map(|_| ()).map_err(|_| ());
                            redis_xadd_status_inc(&config.accounts.stream, status, pipe_accounts);
                            redis_xadd_status_inc(&config.transactions.stream, status, pipe_transactions);

                            Ok::<(), anyhow::Error>(())
                        }
                    });
                }
            },
        };

        while tasks.len() >= config.redis.max_xadd_in_process {
            if let Some(result) = tasks.join_next().await {
                result??;
            }
        }
    };

    tokio::select! {
        Some(signal) = shutdown.next() => {
            anyhow::bail!("{signal} received, force shutdown...");
        }
        result = async move {
            while let Some(result) = tasks.join_next().await {
                result??;
            }
            Ok::<(), anyhow::Error>(())
        } => result?,
    };

    result
}
