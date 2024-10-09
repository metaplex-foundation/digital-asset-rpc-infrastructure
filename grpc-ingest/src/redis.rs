use {
    crate::{
        config::{ConfigIngestStream, REDIS_STREAM_DATA_KEY},
        prom::{
            ingest_tasks_reset, ingest_tasks_total_dec, ingest_tasks_total_inc,
            program_transformer_task_status_inc, redis_xack_inc, redis_xlen_set,
            ProgramTransformerTaskStatusKind,
        },
    },
    das_core::DownloadMetadataInfo,
    futures::future::BoxFuture,
    program_transformers::{AccountInfo, TransactionInfo},
    redis::{
        aio::MultiplexedConnection,
        streams::{
            StreamClaimOptions, StreamClaimReply, StreamId, StreamKey, StreamMaxlen,
            StreamPendingCountReply, StreamReadOptions, StreamReadReply,
        },
        AsyncCommands, ErrorKind as RedisErrorKind, RedisResult, Value as RedisValue,
    },
    solana_sdk::{pubkey::Pubkey, signature::Signature},
    std::{collections::HashMap, sync::Arc},
    tokio::time::{sleep, Duration},
    topograph::{
        executor::{Executor, Nonblock, Tokio},
        prelude::*,
        AsyncHandler,
    },
    tracing::{debug, error, info},
    yellowstone_grpc_proto::{
        convert_from::{
            create_message_instructions, create_meta_inner_instructions, create_pubkey_vec,
        },
        prelude::{SubscribeUpdateAccount, SubscribeUpdateTransaction},
        prost::Message,
    },
};

pub enum IngestStreamJob {
    Process((String, HashMap<String, RedisValue>)),
}

pub struct IngestStreamStop {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    control: tokio::task::JoinHandle<()>,
}

impl IngestStreamStop {
    pub async fn stop(self) -> anyhow::Result<()> {
        let _ = self.shutdown_tx.send(());

        self.control.await?;

        Ok(())
    }
}

type HandlerFn = dyn Fn(HashMap<String, RedisValue>) -> BoxFuture<'static, Result<(), IngestMessageError>>
    + Send
    + Sync;

#[derive(Clone)]
pub struct IngestStreamHandler {
    handler: Arc<HandlerFn>,
    ack_sender: tokio::sync::mpsc::Sender<String>,
    config: Arc<ConfigIngestStream>,
}

impl<'a>
    AsyncHandler<IngestStreamJob, topograph::executor::Handle<'a, IngestStreamJob, Nonblock<Tokio>>>
    for IngestStreamHandler
{
    type Output = ();

    fn handle(
        &self,
        job: IngestStreamJob,
        _handle: topograph::executor::Handle<'a, IngestStreamJob, Nonblock<Tokio>>,
    ) -> impl futures::Future<Output = Self::Output> + Send {
        let handler = Arc::clone(&self.handler);
        let ack_sender = self.ack_sender.clone();
        let config = Arc::clone(&self.config);

        ingest_tasks_total_inc(&config.name);
        async move {
            match job {
                IngestStreamJob::Process((id, msg)) => {
                    match handler(msg).await {
                        Ok(()) => program_transformer_task_status_inc(
                            ProgramTransformerTaskStatusKind::Success,
                        ),
                        Err(IngestMessageError::RedisStreamMessage(e)) => {
                            error!("Failed to process message: {:?}", e);

                            program_transformer_task_status_inc(e.into());
                        }
                        Err(IngestMessageError::DownloadMetadataJson(e)) => {
                            program_transformer_task_status_inc(e.into());
                        }
                        Err(IngestMessageError::ProgramTransformer(e)) => {
                            error!("Failed to process message: {:?}", e);

                            program_transformer_task_status_inc(e.into());
                        }
                    }

                    if let Err(e) = ack_sender.send(id).await {
                        error!("Failed to send ack id to channel: {:?}", e);
                    }

                    ingest_tasks_total_dec(&config.name);
                }
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RedisStreamMessageError {
    #[error("failed to get data (key: {0}) from stream")]
    MissingData(String),
    #[error("invalid data (key: {0}) from stream")]
    InvalidData(String),
    #[error("failed to decode message")]
    Decode(#[from] yellowstone_grpc_proto::prost::DecodeError),
    #[error("received invalid SubscribeUpdateAccount")]
    InvalidSubscribeUpdateAccount,
    #[error("failed to convert pubkey")]
    PubkeyConversion(#[from] std::array::TryFromSliceError),
    #[error("JSON deserialization error: {0}")]
    JsonDeserialization(#[from] serde_json::Error),
}

pub trait RedisStreamMessage<M> {
    fn try_parse_msg(msg: HashMap<String, RedisValue>) -> Result<M, RedisStreamMessageError>;

    fn get_data_as_vec(
        msg: &HashMap<String, RedisValue>,
    ) -> Result<&Vec<u8>, RedisStreamMessageError> {
        let data = msg.get(REDIS_STREAM_DATA_KEY).ok_or_else(|| {
            RedisStreamMessageError::MissingData(REDIS_STREAM_DATA_KEY.to_string())
        })?;

        match data {
            RedisValue::Data(data) => Ok(data),
            _ => Err(RedisStreamMessageError::InvalidData(
                REDIS_STREAM_DATA_KEY.to_string(),
            )),
        }
    }
}

impl RedisStreamMessage<Self> for AccountInfo {
    fn try_parse_msg(msg: HashMap<String, RedisValue>) -> Result<Self, RedisStreamMessageError> {
        let account_data = Self::get_data_as_vec(&msg)?;

        let SubscribeUpdateAccount { account, slot, .. } = Message::decode(account_data.as_ref())?;

        let account =
            account.ok_or_else(|| RedisStreamMessageError::InvalidSubscribeUpdateAccount)?;

        Ok(Self {
            slot,
            pubkey: Pubkey::try_from(account.pubkey.as_slice())?,
            owner: Pubkey::try_from(account.owner.as_slice())?,
            data: account.data,
        })
    }
}

impl RedisStreamMessage<Self> for TransactionInfo {
    fn try_parse_msg(msg: HashMap<String, RedisValue>) -> Result<Self, RedisStreamMessageError> {
        let transaction_data = Self::get_data_as_vec(&msg)?;

        let SubscribeUpdateTransaction { transaction, slot } =
            Message::decode(transaction_data.as_ref())?;

        let transaction = transaction.ok_or_else(|| {
            RedisStreamMessageError::InvalidData(
                "received invalid SubscribeUpdateTransaction".to_string(),
            )
        })?;
        let tx = transaction.transaction.ok_or_else(|| {
            RedisStreamMessageError::InvalidData(
                "received invalid transaction in SubscribeUpdateTransaction".to_string(),
            )
        })?;
        let message = tx.message.ok_or_else(|| {
            RedisStreamMessageError::InvalidData(
                "received invalid message in SubscribeUpdateTransaction".to_string(),
            )
        })?;
        let meta = transaction.meta.ok_or_else(|| {
            RedisStreamMessageError::InvalidData(
                "received invalid meta in SubscribeUpdateTransaction".to_string(),
            )
        })?;

        let mut account_keys = create_pubkey_vec(message.account_keys).map_err(|e| {
            RedisStreamMessageError::Decode(yellowstone_grpc_proto::prost::DecodeError::new(e))
        })?;
        for pubkey in create_pubkey_vec(meta.loaded_writable_addresses).map_err(|e| {
            RedisStreamMessageError::Decode(yellowstone_grpc_proto::prost::DecodeError::new(e))
        })? {
            account_keys.push(pubkey);
        }
        for pubkey in create_pubkey_vec(meta.loaded_readonly_addresses).map_err(|e| {
            RedisStreamMessageError::Decode(yellowstone_grpc_proto::prost::DecodeError::new(e))
        })? {
            account_keys.push(pubkey);
        }

        Ok(Self {
            slot,
            signature: Signature::try_from(transaction.signature.as_slice())?,
            account_keys,
            message_instructions: create_message_instructions(message.instructions).map_err(
                |e| {
                    RedisStreamMessageError::Decode(
                        yellowstone_grpc_proto::prost::DecodeError::new(e),
                    )
                },
            )?,
            meta_inner_instructions: create_meta_inner_instructions(meta.inner_instructions)
                .map_err(|e| {
                    RedisStreamMessageError::Decode(
                        yellowstone_grpc_proto::prost::DecodeError::new(e),
                    )
                })?,
        })
    }
}

impl RedisStreamMessage<Self> for DownloadMetadataInfo {
    fn try_parse_msg(msg: HashMap<String, RedisValue>) -> Result<Self, RedisStreamMessageError> {
        let metadata_data = Self::get_data_as_vec(&msg)?;

        let info: DownloadMetadataInfo = serde_json::from_slice(metadata_data.as_ref())?;

        Ok(info)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum IngestMessageError {
    #[error("Redis stream message parse error: {0}")]
    RedisStreamMessage(#[from] RedisStreamMessageError),
    #[error("Program transformer error: {0}")]
    ProgramTransformer(#[from] program_transformers::error::ProgramTransformerError),
    #[error("Download metadata JSON task error: {0}")]
    DownloadMetadataJson(#[from] das_core::MetadataJsonTaskError),
}

#[derive(Clone, Debug)]
enum AcknowledgeJob {
    Submit(Vec<String>),
}

#[derive(Clone)]
pub struct AcknowledgeHandler {
    config: Arc<ConfigIngestStream>,
    connection: MultiplexedConnection,
}

impl<'a>
    AsyncHandler<AcknowledgeJob, topograph::executor::Handle<'a, AcknowledgeJob, Nonblock<Tokio>>>
    for AcknowledgeHandler
{
    type Output = ();

    fn handle(
        &self,
        job: AcknowledgeJob,
        _handle: topograph::executor::Handle<'a, AcknowledgeJob, Nonblock<Tokio>>,
    ) -> impl futures::Future<Output = Self::Output> + Send {
        let mut connection = self.connection.clone();
        let config = Arc::clone(&self.config);

        let AcknowledgeJob::Submit(ids) = job;

        let count = ids.len();

        async move {
            match redis::pipe()
                .atomic()
                .xack(&config.name, &config.group, &ids)
                .xdel(&config.name, &ids)
                .query_async::<_, redis::Value>(&mut connection)
                .await
            {
                Ok(response) => {
                    debug!(
                        "Acknowledged and deleted message: stream={:?} response={:?} expected={:?}",
                        &config.name, response, count
                    );

                    redis_xack_inc(&config.name, count);
                }
                Err(e) => {
                    error!("Failed to acknowledge or delete message: error={:?}", e);
                }
            }
        }
    }
}

pub struct IngestStream {
    config: Arc<ConfigIngestStream>,
    connection: Option<MultiplexedConnection>,
    handler: Option<Arc<HandlerFn>>,
}

impl IngestStream {
    pub fn build() -> Self {
        Self {
            config: Arc::new(ConfigIngestStream::default()),
            connection: None,
            handler: None,
        }
    }

    pub fn config(mut self, config: ConfigIngestStream) -> Self {
        self.config = Arc::new(config);
        self
    }

    pub fn connection(mut self, connection: MultiplexedConnection) -> Self {
        self.connection = Some(connection);
        self
    }

    pub fn handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(HashMap<String, RedisValue>) -> BoxFuture<'static, Result<(), IngestMessageError>>
            + Send
            + Sync
            + 'static,
    {
        self.handler = Some(Arc::new(handler));
        self
    }

    async fn pending(
        &self,
        connection: &mut MultiplexedConnection,
        start: &str,
    ) -> RedisResult<Option<StreamClaimReply>> {
        let config = Arc::clone(&self.config);

        let pending = redis::cmd("XPENDING")
            .arg(&config.name)
            .arg(&config.group)
            .arg(start)
            .arg("+")
            .arg(config.batch_size)
            .arg(&config.consumer)
            .query_async::<_, StreamPendingCountReply>(connection)
            .await?;
        let ids: Vec<&str> = pending.ids.iter().map(|info| info.id.as_str()).collect();
        let opts = StreamClaimOptions::default();

        let claimed: StreamClaimReply = connection
            .xclaim_options(
                &config.name,
                &config.group,
                &config.consumer,
                100,
                &ids,
                opts,
            )
            .await?;

        if claimed.ids.is_empty() {
            return Ok(None);
        }

        Ok(Some(claimed))
    }

    async fn read(&self, connection: &mut MultiplexedConnection) -> RedisResult<StreamReadReply> {
        let config = &self.config;

        let opts = StreamReadOptions::default()
            .group(&config.group, &config.consumer)
            .count(config.batch_size)
            .block(100);

        connection
            .xread_options(&[&config.name], &[">"], &opts)
            .await
    }

    pub fn start(mut self) -> anyhow::Result<IngestStreamStop> {
        let config = Arc::clone(&self.config);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let connection = self.connection.take().expect("Connection is required");

        let group_create_connection = connection.clone();
        let config_group_create = Arc::clone(&config);

        tokio::task::spawn_blocking(move || {
            let mut connection = group_create_connection.clone();
            let config = Arc::clone(&config_group_create);

            let rt = tokio::runtime::Runtime::new()?;

            rt.block_on(async {
                if let Err(e) = xgroup_create(
                    &mut connection,
                    &config.name,
                    &config.group,
                    &config.consumer,
                )
                .await
                {
                    error!("redis=xgroup_create stream={} err={:?}", config.name, e);
                } else {
                    debug!(
                        "redis=xgroup_create stream={} group={} consumer={}",
                        config.name, config.group, config.consumer
                    );
                }
            });

            Ok::<(), anyhow::Error>(())
        });

        let (ack_tx, mut ack_rx) = tokio::sync::mpsc::channel::<String>(config.xack_buffer_size);
        let (ack_shutdown_tx, ack_shutdown_rx) = tokio::sync::oneshot::channel();

        let xack_executor = Executor::builder(Nonblock(Tokio))
            .max_concurrency(Some(10))
            .build_async(AcknowledgeHandler {
                config: Arc::clone(&config),
                connection: connection.clone(),
            })?;

        let ack = tokio::spawn({
            let config = Arc::clone(&config);
            let mut pending = Vec::new();

            async move {
                let mut shutdown_rx = ack_shutdown_rx;
                let deadline = tokio::time::sleep(config.xack_batch_max_idle);
                tokio::pin!(deadline);

                loop {
                    tokio::select! {
                        Some(id) = ack_rx.recv() => {
                            pending.push(id);
                            let count = pending.len();
                            if count >= config.xack_batch_max_size {

                                let ids = std::mem::take(&mut pending);
                                xack_executor.push(AcknowledgeJob::Submit(ids));
                                deadline.as_mut().reset(tokio::time::Instant::now() + config.xack_batch_max_idle);
                            }
                        },
                        _ = &mut deadline, if !pending.is_empty() => {
                            let ids = std::mem::take(&mut pending);

                            xack_executor.push(AcknowledgeJob::Submit(ids));

                            deadline.as_mut().reset(tokio::time::Instant::now() + config.xack_batch_max_idle);
                        },
                        _ = &mut shutdown_rx => {
                            xack_executor.join_async().await;
                            break;
                        }
                    }
                }
            }
        });

        let handler = self.handler.take().expect("Handler is required");

        ingest_tasks_reset(&config.name);

        let executor = Executor::builder(Nonblock(Tokio))
            .max_concurrency(Some(config.max_concurrency))
            .build_async(IngestStreamHandler {
                handler,
                ack_sender: ack_tx.clone(),
                config: Arc::clone(&config),
            })?;

        let labels = vec![config.name.clone()];
        let (report_shutdown_tx, report_shutdown_rx) = tokio::sync::oneshot::channel();
        let report_thread = tokio::spawn({
            let mut shutdown_rx: tokio::sync::oneshot::Receiver<()> = report_shutdown_rx;
            let connection = connection.clone();
            let config = Arc::clone(&config);

            async move {
                let config = Arc::clone(&config);

                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => {
                            debug!(
                                "redis=report_thread stream={} Shutdown signal received, exiting report loop",
                                config.name
                            );
                            break;
                        },
                        _ = sleep(Duration::from_millis(100)) => {
                            if let Err(e) = report_xlen(connection.clone(), labels.clone()).await {
                                error!("redis=report_xlen err={:?}", e);
                            }

                            debug!(
                                "redis=report_thread stream={} msg=waiting for pending messages",
                                config.name
                            );
                        },
                    }
                }
            }
        });

        let control = tokio::spawn({
            let mut connection = connection.clone();
            async move {
                let config = Arc::clone(&config);

                debug!(
                    "redis=read_stream stream={} Starting read stream task",
                    config.name
                );

                let mut shutdown_rx: tokio::sync::oneshot::Receiver<()> = shutdown_rx;

                let mut start = "-".to_owned();

                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => {
                            debug!(
                                "redis=read_stream stream={} Shutdown signal received, exiting loops",
                                config.name
                            );
                            break;
                        },
                        claimed = self.pending(&mut connection, &start) => {
                            if let Ok(Some(claimed)) = claimed {

                                let ids = claimed.ids.clone();
                                let ids: Vec<&str> = ids.iter().map(|info| info.id.as_str()).collect();

                                info!("redis=claimed stream={} claimed={:?}", config.name, ids.len());


                                    for StreamId { id, map } in claimed.ids.into_iter() {
                                        executor.push(IngestStreamJob::Process((id, map)));
                                    }


                                if let Some(last) = ids.last() {
                                    start = last.to_string();
                                }
                            } else {
                                break;
                            }
                        },
                    }
                }

                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => {
                            debug!(
                                "redis=read_stream stream={} Shutdown signal received, exiting read loop",
                                config.name
                            );
                            break;
                        },
                        result = self.read(&mut connection) => {
                            match result {
                                Ok(reply) => {
                                    let count = reply.keys.len();
                                    debug!(
                                        "redis=xread stream={:?} count={:?}",
                                        &config.name, count
                                    );

                                    for StreamKey { key: _, ids } in reply.keys {
                                        for StreamId { id, map } in ids {
                                            executor.push(IngestStreamJob::Process((id, map)));
                                        }
                                    }
                                }
                                Err(err) => {
                                    error!("redis=xread stream={:?} err={:?}", &config.name, err);
                                }
                            }
                        }
                    }
                }

                debug!("stream={} msg=start shut down ingest stream", config.name);

                executor.join_async().await;

                if let Err(e) = report_shutdown_tx.send(()) {
                    error!("Failed to send report shutdown signal: {:?}", e);
                }

                if let Err(e) = ack_shutdown_tx.send(()) {
                    error!("Failed to send ack shutdown signal: {:?}", e);
                }

                if let Err(e) = ack.await {
                    error!("Error during ack shutdown: {:?}", e);
                }

                if let Err(e) = report_thread.await {
                    error!("Error during report thread shutdown: {:?}", e);
                }

                debug!("stream={} msg=shut down stream", config.name);
            }
        });

        Ok(IngestStreamStop {
            control,
            shutdown_tx,
        })
    }
}

pub async fn report_xlen<C: AsyncCommands>(
    mut connection: C,
    streams: Vec<String>,
) -> anyhow::Result<()> {
    let mut pipe = redis::pipe();
    for stream in &streams {
        pipe.xlen(stream);
    }
    let xlens: Vec<usize> = pipe.query_async(&mut connection).await?;

    for (stream, xlen) in streams.iter().zip(xlens.into_iter()) {
        redis_xlen_set(stream, xlen);
    }

    Ok(())
}

pub async fn xgroup_create<C: AsyncCommands>(
    connection: &mut C,
    name: &str,
    group: &str,
    consumer: &str,
) -> anyhow::Result<()> {
    let result: RedisResult<RedisValue> = connection.xgroup_create_mkstream(name, group, "0").await;
    if let Err(error) = result {
        if !(error.kind() == RedisErrorKind::ExtensionError
            && error.detail() == Some("Consumer Group name already exists")
            && error.code() == Some("BUSYGROUP"))
        {
            return Err(error.into());
        }
    }

    // XGROUP CREATECONSUMER key group consumer
    redis::cmd("XGROUP")
        .arg("CREATECONSUMER")
        .arg(name)
        .arg(group)
        .arg(consumer)
        .query_async(connection)
        .await?;

    Ok(())
}

pub struct TrackedPipeline {
    pipeline: redis::Pipeline,
    counts: HashMap<String, usize>,
}

impl Default for TrackedPipeline {
    fn default() -> Self {
        Self {
            pipeline: redis::pipe(),
            counts: HashMap::new(),
        }
    }
}

type TrackedStreamCounts = HashMap<String, usize>;

impl TrackedPipeline {
    pub fn xadd_maxlen<F, V>(&mut self, key: &str, maxlen: StreamMaxlen, id: F, field: V)
    where
        F: redis::ToRedisArgs,
        V: redis::ToRedisArgs,
    {
        self.pipeline
            .xadd_maxlen(key, maxlen, id, &[(REDIS_STREAM_DATA_KEY, field)]);
        *self.counts.entry(key.to_string()).or_insert(0) += 1;
    }

    pub async fn flush(
        &mut self,
        connection: &mut MultiplexedConnection,
    ) -> Result<TrackedStreamCounts, TrackedStreamCounts> {
        let result: RedisResult<RedisValue> = self.pipeline.atomic().query_async(connection).await;
        let counts = self.counts.clone();
        self.counts.clear();
        self.pipeline.clear();

        match result {
            Ok(_) => Ok(counts),
            Err(_) => Err(counts),
        }
    }

    pub fn size(&self) -> usize {
        self.counts.values().sum()
    }
}
