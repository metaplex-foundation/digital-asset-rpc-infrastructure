use {
    crate::{
        config::ConfigGrpc, prom::redis_xadd_status_inc, redis::TrackedPipeline,
        util::create_shutdown,
    },
    anyhow::Context,
    futures::{channel::mpsc::SendError, stream::StreamExt, Sink, SinkExt},
    redis::streams::StreamMaxlen,
    std::{collections::HashMap, pin::Pin, sync::Arc, time::Duration},
    tokio::{sync::Mutex, time::sleep},
    topograph::{
        executor::{Executor, Nonblock, Tokio},
        prelude::*,
        AsyncHandler,
    },
    tracing::{debug, warn},
    yellowstone_grpc_client::GeyserGrpcClient,
    yellowstone_grpc_proto::{
        geyser::{SubscribeRequest, SubscribeRequestPing, SubscribeUpdate},
        prelude::subscribe_update::UpdateOneof,
        prost::Message,
    },
    yellowstone_grpc_tools::config::GrpcRequestToProto,
};

const PING_ID: i32 = 0;

enum GrpcJob {
    FlushRedisPipe,
    ProcessSubscribeUpdate(Box<SubscribeUpdate>),
}

type SubscribeTx = Pin<Box<dyn Sink<SubscribeRequest, Error = SendError> + Send + Sync>>;

#[derive(Clone)]
pub struct GrpcJobHandler {
    connection: redis::aio::MultiplexedConnection,
    config: Arc<ConfigGrpc>,
    pipe: Arc<Mutex<TrackedPipeline>>,
    subscribe_tx: Arc<Mutex<SubscribeTx>>,
}

impl<'a> AsyncHandler<GrpcJob, topograph::executor::Handle<'a, GrpcJob, Nonblock<Tokio>>>
    for GrpcJobHandler
{
    type Output = ();

    fn handle(
        &self,
        job: GrpcJob,
        handle: topograph::executor::Handle<'a, GrpcJob, Nonblock<Tokio>>,
    ) -> impl futures::Future<Output = Self::Output> + Send + 'a {
        let config = Arc::clone(&self.config);
        let connection = self.connection.clone();
        let pipe = Arc::clone(&self.pipe);

        let subscribe_tx = Arc::clone(&self.subscribe_tx);

        async move {
            match job {
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
                    let transactions_stream = config.transactions.stream.clone();
                    let transactions_stream_maxlen = config.transactions.stream_maxlen;

                    let SubscribeUpdate { update_oneof, .. } = *update;

                    let mut pipe = pipe.lock().await;

                    if let Some(update) = update_oneof {
                        match update {
                            UpdateOneof::Account(account) => {
                                pipe.xadd_maxlen(
                                    &accounts_stream,
                                    StreamMaxlen::Approx(accounts_stream_maxlen),
                                    "*",
                                    account.encode_to_vec(),
                                );

                                debug!(message = "Account update", ?account,);
                            }
                            UpdateOneof::Transaction(transaction) => {
                                pipe.xadd_maxlen(
                                    &transactions_stream,
                                    StreamMaxlen::Approx(transactions_stream_maxlen),
                                    "*",
                                    transaction.encode_to_vec(),
                                );

                                debug!(message = "Transaction update", ?transaction);
                            }
                            UpdateOneof::Ping(_) => {
                                subscribe_tx
                                    .lock()
                                    .await
                                    .send(SubscribeRequest {
                                        ping: Some(SubscribeRequestPing { id: PING_ID }),
                                        ..Default::default()
                                    })
                                    .await
                                    .map_err(|err| {
                                        warn!(message = "Failed to send ping", ?err);
                                    })
                                    .ok();

                                debug!(message = "Ping", id = PING_ID);
                            }
                            UpdateOneof::Pong(pong) => {
                                if pong.id == PING_ID {
                                    debug!(message = "Pong", id = PING_ID);
                                } else {
                                    warn!(message = "Unknown pong id", id = pong.id);
                                }
                            }
                            var => warn!(message = "Unknown update variant", ?var),
                        }
                    }

                    if pipe.size() >= config.redis.pipeline_max_size {
                        handle.push(GrpcJob::FlushRedisPipe);
                    }
                }
            }
        }
    }
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

    let (subscribe_tx, stream) = dragon_mouth_client
        .subscribe_with_request(Some(request))
        .await?;

    tokio::pin!(stream);

    let exec = Executor::builder(Nonblock(Tokio))
        .max_concurrency(Some(config.max_concurrency))
        .build_async(GrpcJobHandler {
            config: Arc::clone(&config),
            connection: connection.clone(),
            pipe: Arc::clone(&pipe),
            subscribe_tx: Arc::new(Mutex::new(Box::pin(subscribe_tx))),
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
