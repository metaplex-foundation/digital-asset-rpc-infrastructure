use {
    crate::{
        config::{ConfigGrpc, ConfigGrpcRequestFilter, ConfigSubscription},
        prom::{grpc_tasks_total_dec, grpc_tasks_total_inc, redis_xadd_status_inc},
        redis::TrackedPipeline,
        util::create_shutdown,
    },
    anyhow::Context,
    futures::{
        stream::{FuturesUnordered, StreamExt},
        SinkExt,
    },
    redis::streams::StreamMaxlen,
    std::{collections::HashMap, sync::Arc, time::Duration},
    tokio::{
        sync::{oneshot, Mutex},
        time::sleep,
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

pub async fn run(config: ConfigGrpc) -> anyhow::Result<()> {
    let redis_client = redis::Client::open(config.redis.url.clone())?;
    let connection = redis_client.get_multiplexed_tokio_connection().await?;

    let mut shutdown = create_shutdown()?;

    let config = Arc::new(config);

    let subscriptions = config.subscriptions.clone();

    let (global_shutdown_tx, mut global_shutdown_rx) = oneshot::channel();
    let global_shutdown_tx = Arc::new(Mutex::new(Some(global_shutdown_tx)));

    let mut subscription_tasks = Vec::new();
    for (label, subscription_config) in subscriptions {
        let subscription = Subscription {
            label,
            config: subscription_config,
        };
        let task = SubscriptionTask::build()
            .config(Arc::clone(&config))
            .connection(connection.clone())
            .subscription(subscription)
            .start(Arc::clone(&global_shutdown_tx))
            .await?;

        subscription_tasks.push(task);
    }

    tokio::select! {
        _ = &mut global_shutdown_rx => {
            warn!(
                target: "grpc2redis",
                action = "global_shutdown_signal_received",
                message = "Global shutdown signal received, stopping all tasks"
            );
        }
        _ = shutdown.next() => {
            warn!(
                target: "grpc2redis",
                action = "shutdown_signal_received",
                message = "Shutdown signal received, waiting for spawned tasks to complete"
            );
        }
    }

    futures::future::join_all(
        subscription_tasks
            .into_iter()
            .map(|task| task.stop())
            .collect::<Vec<_>>(),
    )
    .await
    .into_iter()
    .collect::<anyhow::Result<()>>()?;

    Ok(())
}

pub struct Subscription {
    pub label: String,
    pub config: ConfigSubscription,
}

#[derive(Default)]
pub struct SubscriptionTask {
    pub config: Arc<ConfigGrpc>,
    pub connection: Option<redis::aio::MultiplexedConnection>,
    pub subscription: Option<Subscription>,
}

impl SubscriptionTask {
    pub fn build() -> Self {
        Self::default()
    }

    pub fn config(mut self, config: Arc<ConfigGrpc>) -> Self {
        self.config = config;
        self
    }

    pub fn subscription(mut self, subscription: Subscription) -> Self {
        self.subscription = Some(subscription);
        self
    }

    pub fn connection(mut self, connection: redis::aio::MultiplexedConnection) -> Self {
        self.connection = Some(connection);
        self
    }

    pub async fn start(
        mut self,
        global_shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    ) -> anyhow::Result<SubscriptionTaskStop> {
        let config = Arc::clone(&self.config);
        let connection = self
            .connection
            .take()
            .expect("Redis Connection is required");

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        let subscription = self.subscription.take().expect("Subscription is required");
        let label = subscription.label.clone();
        let subscription_config = Arc::new(subscription.config);
        let connection = connection.clone();

        let ConfigSubscription { stream, filter } = subscription_config.as_ref().clone();

        let stream_config = Arc::new(stream.clone());
        let mut req_accounts = HashMap::with_capacity(1);
        let mut req_transactions = HashMap::with_capacity(1);

        let ConfigGrpcRequestFilter {
            accounts,
            transactions,
        } = filter;

        if let Some(accounts) = accounts {
            req_accounts.insert(label.clone(), accounts.to_proto());
        }

        if let Some(transactions) = transactions {
            req_transactions.insert(label.clone(), transactions.to_proto());
        }

        let request = SubscribeRequest {
            accounts: req_accounts,
            transactions: req_transactions,
            commitment: Some(config.geyser.commitment.to_proto().into()),
            ..Default::default()
        };

        let pipes: Vec<_> = (0..stream_config.pipeline_count)
            .map(|_| Arc::new(Mutex::new(TrackedPipeline::default())))
            .collect();
        let mut tasks = FuturesUnordered::new();

        let mut dragon_mouth_client =
            GeyserGrpcClient::build_from_shared(config.geyser.endpoint.clone())?
                .x_token(config.geyser.x_token.clone())?
                .connect_timeout(Duration::from_secs(config.geyser.connect_timeout))
                .timeout(Duration::from_secs(config.geyser.timeout))
                .connect()
                .await
                .context("failed to connect to gRPC")?;

        let (mut subscribe_tx, stream) = dragon_mouth_client
            .subscribe_with_request(Some(request))
            .await?;
        let global_shutdown_tx = Arc::clone(&global_shutdown_tx);

        let control = tokio::spawn({
            async move {
                tokio::pin!(stream);

                let mut flush_handles = Vec::new();
                let mut shutdown_senders = Vec::new();

                for pipe in &pipes {
                    let pipe = Arc::clone(pipe);
                    let stream_config = Arc::clone(&stream_config);
                    let label = label.clone();
                    let mut connection = connection.clone();
                    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

                    let flush_handle = tokio::spawn(async move {
                        loop {
                            tokio::select! {
                                _ = sleep(stream_config.pipeline_max_idle) => {
                                    let mut pipe = pipe.lock().await;
                                    let flush = pipe.flush(&mut connection).await;

                                    let status = flush.as_ref().map(|_| ()).map_err(|_| ());
                                    let count = flush.as_ref().unwrap_or_else(|count| count);

                                    debug!(target: "grpc2redis", action = "flush_redis_pip_deadline", stream = ?stream_config.name, status = ?status, count = ?count);
                                    redis_xadd_status_inc(&stream_config.name, &label, status, *count);
                                }
                                _ = &mut shutdown_rx => {
                                    let mut pipe = pipe.lock().await;
                                    let flush = pipe.flush(&mut connection).await;

                                    let status = flush.as_ref().map(|_| ()).map_err(|_| ());
                                    let count = flush.as_ref().unwrap_or_else(|count| count);

                                    debug!(target: "grpc2redis", action = "final_flush_redis_pipe", stream = ?stream_config.name, status = ?status, count = ?count);
                                    redis_xadd_status_inc(&stream_config.name, &label, status, *count);
                                    break;
                                }
                            }
                        }
                    });

                    flush_handles.push(flush_handle);
                    shutdown_senders.push(shutdown_tx);
                }

                let mut current_pipe_index = 0;

                loop {
                    tokio::select! {
                        event = stream.next() => {
                            match event {
                                Some(Ok(msg)) => {
                                    match msg.update_oneof {
                                        Some(UpdateOneof::Account(_)) | Some(UpdateOneof::Transaction(_)) => {
                                            if tasks.len() >= stream_config.max_concurrency {
                                                tasks.next().await;
                                            }
                                            grpc_tasks_total_inc(&label, &stream_config.name);

                                            tasks.push(tokio::spawn({
                                                let pipe = Arc::clone(&pipes[current_pipe_index]);
                                                let label = label.clone();
                                                let stream_config = Arc::clone(&stream_config);

                                                async move {
                                                    let stream = stream_config.name.clone();
                                                    let stream_maxlen = stream_config.max_len;

                                                    let SubscribeUpdate { update_oneof, .. } = msg;

                                                    let mut pipe = pipe.lock().await;

                                                    if let Some(update) = update_oneof {
                                                        match update {
                                                            UpdateOneof::Account(account) => {
                                                                pipe.xadd_maxlen(
                                                                    &stream.to_string(),
                                                                    StreamMaxlen::Approx(stream_maxlen),
                                                                    "*",
                                                                    account.encode_to_vec(),
                                                                );
                                                            }

                                                            UpdateOneof::Transaction(transaction) => {
                                                                if let Some(transaction) = &transaction.transaction {
                                                                    if let Some(meta) = &transaction.meta {
                                                                        if meta.err.is_some() {
                                                                            return;
                                                                        }
                                                                    }
                                                                }

                                                                pipe.xadd_maxlen(
                                                                    &stream.to_string(),
                                                                    StreamMaxlen::Approx(stream_maxlen),
                                                                    "*",
                                                                    transaction.encode_to_vec(),
                                                                );
                                                            }
                                                            _ => {
                                                                warn!(target: "grpc2redis", action = "unknown_update_variant", label = ?label, message = "Unknown update variant");
                                                            }
                                                        }
                                                    }

                                                    grpc_tasks_total_dec(&label, &stream_config.name);
                                                }
                                            }));

                                            current_pipe_index = (current_pipe_index + 1) % pipes.len();
                                        }
                                        Some(UpdateOneof::Ping(_)) => {
                                            let ping = subscribe_tx
                                                .send(SubscribeRequest {
                                                    ping: Some(SubscribeRequestPing { id: PING_ID }),
                                                    ..Default::default()
                                                })
                                                .await;

                                            match ping {
                                                Ok(_) => {
                                                    debug!(target: "grpc2redis", action = "send_ping", message = "Ping sent successfully", id = PING_ID);
                                                }
                                                Err(err) => {
                                                    warn!(target: "grpc2redis", action = "send_ping_failed", message = "Failed to send ping", ?err, id = PING_ID);
                                                }
                                            }
                                        }
                                        Some(UpdateOneof::Pong(pong)) => {
                                            if pong.id == PING_ID {
                                                debug!(target: "grpc2redis", action = "receive_pong", message = "Pong received", id = PING_ID);
                                            } else {
                                                warn!(target: "grpc2redis", action = "receive_unknown_pong", message = "Unknown pong id received", id = pong.id);
                                            }
                                        }
                                        _ => {
                                            warn!(target: "grpc2redis", action = "unknown_update_variant", message = "Unknown update variant");
                                        }
                                    }
                                }
                                None | Some(Err(_)) => {
                                    warn!(target: "grpc2redis", action = "stream_closed", message = "Stream closed, stopping subscription task", ?label);

                                    let mut global_shutdown_tx = global_shutdown_tx.lock().await;
                                    if let Some(global_shutdown_tx) = global_shutdown_tx.take() {
                                        let _ = global_shutdown_tx.send(());
                                    }
                                }
                            }
                        }
                        _ = &mut shutdown_rx => {
                            debug!(target: "grpc2redis", action = "shutdown_signal_received", message = "Shutdown signal received, stopping subscription task", ?label);
                            break;
                        }
                    }
                }

                debug!(target: "grpc2redis", action = "shutdown_subscription_task", message = "Subscription task stopped", ?label);
                while (tasks.next().await).is_some() {}

                for shutdown_tx in shutdown_senders {
                    debug!(target: "grpc2redis", action = "send_shutdown_signal", message = "Sending shutdown signal to flush handles");
                    let _ = shutdown_tx.send(());
                }

                debug!(target: "grpc2redis", action = "wait_flush_handles", message = "Waiting for flush handles to complete");
                futures::future::join_all(flush_handles).await;
            }
        });

        Ok(SubscriptionTaskStop {
            shutdown_tx,
            control,
        })
    }
}

#[derive(Debug)]
pub struct SubscriptionTaskStop {
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
    pub control: tokio::task::JoinHandle<()>,
}

impl SubscriptionTaskStop {
    pub async fn stop(self) -> anyhow::Result<()> {
        self.shutdown_tx
            .send(())
            .map_err(|_| anyhow::anyhow!("Failed to send shutdown signal"))?;

        self.control.await?;

        Ok(())
    }
}
