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
    FlushRedisPipe(Arc<Mutex<TrackedPipeline>>, MultiplexedConnection),
    ProcessSubscribeUpdate(Arc<Mutex<TrackedPipeline>>, SubscribeUpdate),
}

pub async fn run(config: ConfigGrpc) -> anyhow::Result<()> {
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

    let (_subscribe_tx, mut stream) = dragon_mouth_client
        .subscribe_with_request(Some(request))
        .await?;

    let pool_config = Arc::clone(&config);

    let exec = Executor::builder(Nonblock(Tokio))
        .num_threads(None)
        .build(move |update, _| {
            let config = pool_config.clone();

            async move {
                match update {
                    GrpcJob::FlushRedisPipe(pipe, connection) => {
                        let mut pipe = pipe.lock().await;
                        let mut connection = connection;

                        let flush = pipe.flush(&mut connection).await;

                        let status = flush.as_ref().map(|_| ()).map_err(|_| ());
                        let counts = flush.as_ref().unwrap_or_else(|counts| counts);

                        for (stream, count) in counts.iter() {
                            redis_xadd_status_inc(stream, status, count);
                        }

                        debug!(message = "Redis pipe flushed", ?status, ?counts);
                    }
                    GrpcJob::ProcessSubscribeUpdate(pipe, update) => {
                        let accounts_stream = config.accounts.stream.clone();
                        let accounts_stream_maxlen = config.accounts.stream_maxlen;
                        let accounts_stream_data_key = config.accounts.stream_data_key.clone();
                        let transactions_stream = config.transactions.stream.clone();
                        let transactions_stream_maxlen = config.transactions.stream_maxlen;
                        let transactions_stream_data_key =
                            config.transactions.stream_data_key.clone();

                        let SubscribeUpdate { update_oneof, .. } = update;
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

                                    debug!(
                                        message = "Account update",
                                        ?account,
                                        ?accounts_stream,
                                        ?accounts_stream_maxlen
                                    );
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

                                    debug!(message = "Transaction update", ?transaction,);
                                }
                                var => warn!(message = "Unknown update variant", ?var),
                            }
                        }
                    }
                }
            }
        })?;

    let deadline_pipe = Arc::clone(&pipe);
    let deadline_config = Arc::clone(&config);
    let deadline_connection = connection.clone();

    loop {
        tokio::select! {
            _ = sleep(deadline_config.redis.pipeline_max_idle) => {
                exec.push(GrpcJob::FlushRedisPipe(deadline_pipe.clone(), deadline_connection.clone()));
            }
            Some(Ok(msg)) = stream.next() => {
                debug!(message = "Received gRPC message", ?msg);
                exec.push(GrpcJob::ProcessSubscribeUpdate(Arc::clone(&pipe), msg));
            }
            _ = shutdown.next() => {
                exec.push(GrpcJob::FlushRedisPipe(Arc::clone(&pipe), connection.clone()));
                break;
            }
        }
    }

    exec.join_async().await;

    Ok(())
}
