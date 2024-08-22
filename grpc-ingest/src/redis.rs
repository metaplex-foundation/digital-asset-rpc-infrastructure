use {
    crate::{
        config::{
            ConfigIngestStream, ConfigIngesterRedis, ConfigIngesterRedisStreamType,
            REDIS_STREAM_DATA_KEY,
        },
        prom::{
            program_transformer_task_status_inc, redis_xack_inc, redis_xlen_set,
            ProgramTransformerTaskStatusKind,
        },
    },
    das_core::DownloadMetadataInfo,
    futures::future::{BoxFuture, Fuse, FutureExt},
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
    std::{
        collections::HashMap,
        convert::Infallible,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    },
    tokio::{
        sync::mpsc,
        task::JoinSet,
        time::{sleep, Duration, Instant},
    },
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
        // self.executor.join_async().await;

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
                    info!("Acknowledged and deleted message: response={:?}", response);

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
    config: ConfigIngestStream,
    connection: Option<MultiplexedConnection>,
    handler: Option<Arc<HandlerFn>>,
}

impl IngestStream {
    pub fn build() -> Self {
        Self {
            config: ConfigIngestStream::default(),
            connection: None,
            handler: None,
        }
    }

    pub fn config(mut self, config: ConfigIngestStream) -> Self {
        self.config = config;
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

    pub fn start(mut self) -> anyhow::Result<IngestStreamStop> {
        let config = Arc::new(self.config);

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
                    error!("Failed to create group: {:?}", e);
                } else {
                    debug!(
                        "Group created successfully: name={}, group={}, consumer={}",
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

        tokio::spawn({
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

        let executor = Executor::builder(Nonblock(Tokio))
            .max_concurrency(Some(config.max_concurrency))
            .build_async(IngestStreamHandler {
                handler,
                ack_sender: ack_tx.clone(),
            })?;

        let connection_report = connection.clone();
        let streams_report = vec![config.name.clone()];

        tokio::spawn(async move {
            loop {
                let connection = connection_report.clone();
                let streams = streams_report.clone();

                if let Err(e) = report_xlen(connection, streams).await {
                    error!("Failed to report xlen: {:?}", e);
                }

                sleep(Duration::from_millis(100)).await;
            }
        });

        let config_read = Arc::clone(&config);
        let mut connection_read = connection.clone();

        let (read_shutdown_tx, read_shutdown_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            debug!("Starting read stream task name={}", config_read.name);

            let mut shutdown_rx = read_shutdown_rx;

            loop {
                if shutdown_rx.try_recv().is_ok() {
                    debug!(
                        "Shutdown signal received, exiting prefetch loop name={}",
                        config_read.name
                    );
                    break;
                }

                if let Ok(pending) = redis::cmd("XPENDING")
                    .arg(&config_read.name)
                    .arg(&config_read.group)
                    .arg("-")
                    .arg("+")
                    .arg(config_read.batch_size)
                    .arg(&config_read.consumer)
                    .query_async::<_, StreamPendingCountReply>(&mut connection_read)
                    .await
                {
                    if pending.ids.is_empty() {
                        debug!(
                            "No pending messages stream={} consumer={} group={}",
                            config_read.name, config_read.consumer, config_read.group
                        );
                        break;
                    }

                    let ids: Vec<&str> = pending.ids.iter().map(|info| info.id.as_str()).collect();
                    let claim_opts = StreamClaimOptions::default();

                    let claimed: RedisResult<StreamClaimReply> = connection_read
                        .xclaim_options(
                            &config_read.name,
                            &config_read.group,
                            &config_read.consumer,
                            20,
                            &ids,
                            claim_opts,
                        )
                        .await;

                    if let Ok(claimed) = claimed {
                        for StreamId { id, map } in claimed.ids {
                            executor.push(IngestStreamJob::Process((id, map)));
                        }
                    }
                }
            }

            loop {
                if shutdown_rx.try_recv().is_ok() {
                    debug!(
                        "Shutdown signal received, exiting read loop name={}",
                        config_read.name
                    );
                    break;
                }

                let opts = StreamReadOptions::default()
                    .group(&config_read.group, &config_read.consumer)
                    .count(config_read.batch_size)
                    .block(100);

                let result: RedisResult<StreamReadReply> = connection_read
                    .xread_options(&[&config_read.name], &[">"], &opts)
                    .await;

                match result {
                    Ok(reply) => {
                        let count = reply.keys.len();
                        info!("Reading and processing: count={:?}", count);

                        for StreamKey { key: _, ids } in reply.keys {
                            for StreamId { id, map } in ids {
                                executor.push(IngestStreamJob::Process((id, map)));
                            }
                        }
                    }
                    Err(err) => {
                        error!("Error reading from stream: {:?}", err);
                    }
                }
            }
        });

        let control = tokio::spawn(async move {
            let mut shutdown_rx: tokio::sync::oneshot::Receiver<()> = shutdown_rx;
            debug!("Starting ingest stream name={}", config.name);

            tokio::select! {
                _ = &mut shutdown_rx => {
                    info!("Shut down ingest stream name={}", config.name);

                    let _ = read_shutdown_tx.send(());
                    let _ = ack_shutdown_tx.send(());
                }
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
pub async fn metrics_xlen<C: AsyncCommands>(
    mut connection: C,
    streams: &[String],
) -> anyhow::Result<Infallible> {
    loop {
        let mut pipe = redis::pipe();
        for stream in streams {
            pipe.xlen(stream);
        }
        let xlens: Vec<usize> = pipe.query_async(&mut connection).await?;

        for (stream, xlen) in streams.iter().zip(xlens.into_iter()) {
            redis_xlen_set(stream, xlen);
        }

        sleep(Duration::from_millis(100)).await;
    }
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

#[derive(Debug)]
struct RedisStreamInfo {
    group: String,
    consumer: String,
    stream_name: String,
    stream_type: ConfigIngesterRedisStreamType,
    xack_batch_max_size: usize,
    xack_batch_max_idle: Duration,
    xack_max_in_process: usize,
}

#[derive(Debug)]
pub enum ProgramTransformerInfo {
    Account(AccountInfo),
    Transaction(TransactionInfo),
    MetadataJson(DownloadMetadataInfo),
}

#[derive(Debug)]
pub struct RedisStreamMessageInfo {
    id: String,
    data: ProgramTransformerInfo,
    ack_tx: mpsc::UnboundedSender<String>,
}

impl RedisStreamMessageInfo {
    fn parse(
        stream: &RedisStreamInfo,
        StreamId { id, map }: StreamId,
        ack_tx: mpsc::UnboundedSender<String>,
    ) -> anyhow::Result<Self> {
        let to_anyhow = |error: String| anyhow::anyhow!(error);

        let data = match map.get(REDIS_STREAM_DATA_KEY) {
            Some(RedisValue::Data(vec)) => match stream.stream_type {
                ConfigIngesterRedisStreamType::Account => {
                    let SubscribeUpdateAccount { account, slot, .. } =
                        Message::decode(vec.as_ref())?;

                    let account = account.ok_or_else(|| {
                        anyhow::anyhow!("received invalid SubscribeUpdateAccount")
                    })?;

                    ProgramTransformerInfo::Account(AccountInfo {
                        slot,
                        pubkey: Pubkey::try_from(account.pubkey.as_slice())?,
                        owner: Pubkey::try_from(account.owner.as_slice())?,
                        data: account.data,
                    })
                }
                ConfigIngesterRedisStreamType::Transaction => {
                    let SubscribeUpdateTransaction { transaction, slot } =
                        Message::decode(vec.as_ref())?;

                    let transaction = transaction.ok_or_else(|| {
                        anyhow::anyhow!("received invalid SubscribeUpdateTransaction")
                    })?;
                    let tx = transaction.transaction.ok_or_else(|| {
                        anyhow::anyhow!(
                            "received invalid transaction in SubscribeUpdateTransaction"
                        )
                    })?;
                    let message = tx.message.ok_or_else(|| {
                        anyhow::anyhow!("received invalid message in SubscribeUpdateTransaction")
                    })?;
                    let meta = transaction.meta.ok_or_else(|| {
                        anyhow::anyhow!("received invalid meta in SubscribeUpdateTransaction")
                    })?;

                    let mut account_keys =
                        create_pubkey_vec(message.account_keys).map_err(to_anyhow)?;
                    for pubkey in
                        create_pubkey_vec(meta.loaded_writable_addresses).map_err(to_anyhow)?
                    {
                        account_keys.push(pubkey);
                    }
                    for pubkey in
                        create_pubkey_vec(meta.loaded_readonly_addresses).map_err(to_anyhow)?
                    {
                        account_keys.push(pubkey);
                    }

                    ProgramTransformerInfo::Transaction(TransactionInfo {
                        slot,
                        signature: Signature::try_from(transaction.signature.as_slice())?,
                        account_keys,
                        message_instructions: create_message_instructions(message.instructions)
                            .map_err(to_anyhow)?,
                        meta_inner_instructions: create_meta_inner_instructions(
                            meta.inner_instructions,
                        )
                        .map_err(to_anyhow)?,
                    })
                }
                ConfigIngesterRedisStreamType::MetadataJson => {
                    let info: DownloadMetadataInfo = serde_json::from_slice(vec.as_ref())?;

                    ProgramTransformerInfo::MetadataJson(info)
                }
            },
            Some(_) => anyhow::bail!(
                "invalid data (key: {:?}) from stream {:?}",
                REDIS_STREAM_DATA_KEY,
                stream.stream_name
            ),
            None => anyhow::bail!(
                "failed to get data (key: {:?}) from stream {:?}",
                REDIS_STREAM_DATA_KEY,
                stream.stream_name
            ),
        };
        Ok(Self { id, data, ack_tx })
    }

    pub const fn get_data(&self) -> &ProgramTransformerInfo {
        &self.data
    }

    pub fn ack(self) -> anyhow::Result<()> {
        self.ack_tx
            .send(self.id)
            .map_err(|_error| anyhow::anyhow!("failed to send message to ack channel"))
    }
}

#[derive(Debug)]
pub struct RedisStream {
    shutdown: Arc<AtomicBool>,
    messages_rx: mpsc::Receiver<RedisStreamMessageInfo>,
}

#[allow(dead_code)]
async fn run_ack(
    stream: Arc<RedisStreamInfo>,
    connection: MultiplexedConnection,
    mut ack_rx: mpsc::UnboundedReceiver<String>,
) -> anyhow::Result<()> {
    let mut ids = vec![];
    let deadline = sleep(stream.xack_batch_max_idle);
    tokio::pin!(deadline);
    let mut tasks = JoinSet::new();

    let result = loop {
        let terminated = tokio::select! {
            msg = ack_rx.recv() => match msg {
                Some(msg) => {
                    ids.push(msg);
                    if ids.len() < stream.xack_batch_max_size {
                        continue;
                    }
                    false
                }
                None => true,
            },
            _ = &mut deadline => false,
        };

        let ids = std::mem::take(&mut ids);
        deadline
            .as_mut()
            .reset(Instant::now() + stream.xack_batch_max_idle);
        if !ids.is_empty() {
            tasks.spawn({
                let stream = Arc::clone(&stream);
                let mut connection = connection.clone();
                async move {
                    match redis::pipe()
                        .atomic()
                        .xack(&stream.stream_name, &stream.group, &ids)
                        .xdel(&stream.stream_name, &ids)
                        .query_async::<_, redis::Value>(&mut connection)
                        .await
                    {
                        Ok(_) => {
                            info!("Acknowledged and deleted idle messages: {:?}", ids);
                            redis_xack_inc(&stream.stream_name, ids.len());
                        }
                        Err(e) => {
                            error!("Failed to acknowledge or delete idle messages: {:?}", e);
                        }
                    }
                    redis_xack_inc(&stream.stream_name, ids.len());
                    Ok::<(), anyhow::Error>(())
                }
            });
            while tasks.len() >= stream.xack_max_in_process {
                if let Some(result) = tasks.join_next().await {
                    result??;
                }
            }
        }

        if terminated {
            break Ok(());
        }
    };

    while let Some(result) = tasks.join_next().await {
        result??;
    }

    result
}

impl RedisStream {
    pub async fn new(
        config: ConfigIngesterRedis,
        mut connection: MultiplexedConnection,
    ) -> anyhow::Result<(Self, Fuse<BoxFuture<'static, anyhow::Result<()>>>)> {
        // create group with consumer per stream
        for stream in config.streams.iter() {
            xgroup_create(
                &mut connection,
                stream.stream,
                &config.group,
                &config.consumer,
            )
            .await?;
        }

        // shutdown flag
        let shutdown = Arc::new(AtomicBool::new(false));

        // create stream info wrapped by Arc
        let mut ack_tasks = vec![];
        let streams = config
            .streams
            .iter()
            .map(|stream| {
                let (ack_tx, ack_rx) = mpsc::unbounded_channel();
                let info = Arc::new(RedisStreamInfo {
                    group: config.group.clone(),
                    consumer: config.consumer.clone(),
                    stream_name: stream.stream.to_string(),
                    stream_type: stream.stream_type,
                    xack_batch_max_size: stream.xack_batch_max_size,
                    xack_batch_max_idle: stream.xack_batch_max_idle,
                    xack_max_in_process: stream.xack_max_in_process,
                });
                ack_tasks.push((Arc::clone(&info), ack_rx));
                (stream.stream, (ack_tx, info))
            })
            .collect::<HashMap<_, _>>();

        // spawn xack tasks
        let ack_jh_vec = ack_tasks
            .into_iter()
            .map(|(stream, ack_rx)| {
                let connection = connection.clone();
                tokio::spawn(async move { Self::run_ack(stream, connection, ack_rx).await })
            })
            .collect::<Vec<_>>();

        // spawn prefetch task
        let (messages_tx, messages_rx) = mpsc::channel(config.prefetch_queue_size);
        let jh_prefetch = tokio::spawn({
            let shutdown = Arc::clone(&shutdown);
            async move { Self::run_prefetch(config, streams, connection, messages_tx, shutdown).await }
        });

        // merge spawned xack / prefetch tasks
        let spawned_tasks = async move {
            jh_prefetch.await??;
            for jh in ack_jh_vec.into_iter() {
                jh.await??;
            }
            Ok::<(), anyhow::Error>(())
        };

        Ok((
            Self {
                shutdown,
                messages_rx,
            },
            spawned_tasks.boxed().fuse(),
        ))
    }

    pub async fn recv(&mut self) -> Option<RedisStreamMessageInfo> {
        self.messages_rx.recv().await
    }

    pub fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        tokio::spawn(async move { while self.messages_rx.recv().await.is_some() {} });
    }

    async fn run_prefetch(
        config: ConfigIngesterRedis,
        streams: HashMap<&str, (mpsc::UnboundedSender<String>, Arc<RedisStreamInfo>)>,
        mut connection: MultiplexedConnection,
        messages_tx: mpsc::Sender<RedisStreamMessageInfo>,
        shutdown: Arc<AtomicBool>,
    ) -> anyhow::Result<()> {
        // read pending first
        for (ack_tx, stream) in streams.values() {
            let mut start = "-".to_owned();
            while !shutdown.load(Ordering::Relaxed) {
                let StreamPendingCountReply { ids: pending_ids } = redis::cmd("XPENDING")
                    .arg(&stream.stream_name)
                    .arg(&stream.group)
                    .arg(&start)
                    .arg("+")
                    .arg(config.xpending_max)
                    .arg(&stream.consumer) // we can't use `xpending_count` because it doesn't support `consumer` filter
                    .query_async(&mut connection)
                    .await?;

                // drop first item if we do not start from the beginning
                let used_ids = if start == "-" { 0.. } else { 1.. };
                let ids_str = pending_ids[used_ids]
                    .iter()
                    .map(|pending| pending.id.as_str())
                    .collect::<Vec<_>>();

                // check that we fetched all pendings and update start
                match pending_ids.last() {
                    Some(id) => {
                        if id.id == start {
                            break;
                        } else {
                            start = id.id.clone();
                        }
                    }
                    None => break,
                }

                let StreamClaimReply { ids: pendings } = connection
                    .xclaim(
                        &stream.stream_name,
                        &stream.group,
                        &stream.consumer,
                        0,
                        &ids_str,
                    )
                    .await?;
                for pending in pendings {
                    let item = RedisStreamMessageInfo::parse(stream, pending, ack_tx.clone())?;
                    messages_tx.send(item).await.map_err(|_error| {
                        anyhow::anyhow!("failed to send item to prefetch channel")
                    })?;
                }
            }
        }

        // exit if need to handle only pending
        if config.xpending_only {
            return Ok(());
        }

        let streams_keys = streams.keys().collect::<Vec<_>>();
        let streams_ids = (0..streams_keys.len()).map(|_| ">").collect::<Vec<_>>();

        while !shutdown.load(Ordering::Relaxed) {
            let opts = StreamReadOptions::default()
                .count(config.xreadgroup_max)
                .group(&config.group, &config.consumer);
            let results: StreamReadReply = connection
                .xread_options(&streams_keys, &streams_ids, &opts)
                .await?;

            if results.keys.is_empty() {
                sleep(Duration::from_millis(5)).await;
                continue;
            }

            for StreamKey { key, ids } in results.keys {
                let (ack_tx, stream) = match streams.get(key.as_str()) {
                    Some(value) => value,
                    None => anyhow::bail!("unknown stream: {:?}", key),
                };

                for id in ids {
                    let item = RedisStreamMessageInfo::parse(stream, id, ack_tx.clone())?;
                    messages_tx.send(item).await.map_err(|_error| {
                        anyhow::anyhow!("failed to send item to prefetch channel")
                    })?;
                }
            }
        }

        Ok(())
    }

    async fn run_ack(
        stream: Arc<RedisStreamInfo>,
        connection: MultiplexedConnection,
        mut ack_rx: mpsc::UnboundedReceiver<String>,
    ) -> anyhow::Result<()> {
        let mut ids = vec![];
        let deadline = sleep(stream.xack_batch_max_idle);
        tokio::pin!(deadline);
        let mut tasks = JoinSet::new();

        let result = loop {
            let terminated = tokio::select! {
                msg = ack_rx.recv() => match msg {
                    Some(msg) => {
                        ids.push(msg);
                        if ids.len() < stream.xack_batch_max_size {
                            continue;
                        }
                        false
                    }
                    None => true,
                },
                _ = &mut deadline => false,
            };

            let ids = std::mem::take(&mut ids);
            deadline
                .as_mut()
                .reset(Instant::now() + stream.xack_batch_max_idle);
            if !ids.is_empty() {
                tasks.spawn({
                    let stream = Arc::clone(&stream);
                    let mut connection = connection.clone();
                    async move {
                        match redis::pipe()
                            .atomic()
                            .xack(&stream.stream_name, &stream.group, &ids)
                            .xdel(&stream.stream_name, &ids)
                            .query_async::<_, redis::Value>(&mut connection)
                            .await
                        {
                            Ok(info) => {
                                info!("Acknowledged and deleted idle messages: {:?}", info);
                                redis_xack_inc(&stream.stream_name, ids.len());
                            }
                            Err(e) => {
                                error!("Failed to acknowledge or delete idle messages: {:?}", e);
                            }
                        }
                        redis_xack_inc(&stream.stream_name, ids.len());
                        Ok::<(), anyhow::Error>(())
                    }
                });
                while tasks.len() >= stream.xack_max_in_process {
                    if let Some(result) = tasks.join_next().await {
                        result??;
                    }
                }
            }

            if terminated {
                break Ok(());
            }
        };

        while let Some(result) = tasks.join_next().await {
            result??;
        }

        result
    }
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
