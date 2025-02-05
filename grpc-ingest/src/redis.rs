use {
    crate::{
        config::{ConfigIngestStream, REDIS_STREAM_DATA_KEY},
        prom::{
            ack_tasks_total_dec, ack_tasks_total_inc, download_metadata_json_task_status_count_inc,
            ingest_job_time_set, ingest_tasks_total_dec, ingest_tasks_total_inc,
            program_transformer_task_status_inc, redis_xack_inc, redis_xlen_set, redis_xread_inc,
            ProgramTransformerTaskStatusKind,
        },
    },
    das_core::{
        DownloadMetadata, DownloadMetadataInfo, DownloadMetadataJsonRetryConfig,
        FetchMetadataJsonError, MetadataJsonTaskError, StatusCode,
    },
    futures::future::BoxFuture,
    program_transformers::{AccountInfo, ProgramTransformer, TransactionInfo},
    redis::{
        aio::MultiplexedConnection,
        streams::{StreamId, StreamKey, StreamMaxlen, StreamReadOptions, StreamReadReply},
        AsyncCommands, ErrorKind as RedisErrorKind, RedisResult, Value as RedisValue,
    },
    solana_sdk::{pubkey::Pubkey, signature::Signature},
    std::{collections::HashMap, marker::PhantomData, sync::Arc},
    tokio::{
        task::JoinSet,
        time::{sleep, Duration},
    },
    tracing::{debug, error, warn},
    yellowstone_grpc_proto::{
        convert_from::{
            create_message_instructions, create_meta_inner_instructions, create_pubkey_vec,
        },
        prelude::{SubscribeUpdateAccount, SubscribeUpdateTransaction},
        prost::Message,
    },
};

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

        let account = account.ok_or(RedisStreamMessageError::InvalidSubscribeUpdateAccount)?;

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

pub struct IngestStreamStop {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    control: tokio::task::JoinHandle<()>,
}

impl IngestStreamStop {
    pub async fn stop(self) -> anyhow::Result<()> {
        self.shutdown_tx
            .send(())
            .map_err(|_| anyhow::anyhow!("Failed to send shutdown signal"))?;

        self.control.await?;

        Ok(())
    }
}

pub trait MessageHandler: Send + Sync + Clone + 'static {
    fn handle(
        &self,
        input: HashMap<String, RedisValue>,
    ) -> BoxFuture<'static, Result<(), IngestMessageError>>;
}

pub struct DownloadMetadataJsonHandle(Arc<DownloadMetadata>, Arc<DownloadMetadataJsonRetryConfig>);

impl MessageHandler for DownloadMetadataJsonHandle {
    fn handle(
        &self,
        input: HashMap<String, RedisValue>,
    ) -> BoxFuture<'static, Result<(), IngestMessageError>> {
        let download_metadata = Arc::clone(&self.0);
        let download_config = Arc::clone(&self.1);

        Box::pin(async move {
            let info = DownloadMetadataInfo::try_parse_msg(input)?;
            let response = download_metadata
                .handle_download(&info, download_config)
                .await;
            let status =
                if let Err(MetadataJsonTaskError::Fetch(FetchMetadataJsonError::Response {
                    status: StatusCode::Code(code),
                    ..
                })) = response
                {
                    code.as_u16()
                } else {
                    200
                };

            download_metadata_json_task_status_count_inc(status);

            response.map_err(IngestMessageError::DownloadMetadataJson)
        })
    }
}

impl DownloadMetadataJsonHandle {
    pub const fn new(
        download_metadata: Arc<DownloadMetadata>,
        config: Arc<DownloadMetadataJsonRetryConfig>,
    ) -> Self {
        Self(download_metadata, config)
    }
}

impl Clone for DownloadMetadataJsonHandle {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0), Arc::clone(&self.1))
    }
}

pub struct AccountHandle(Arc<ProgramTransformer>);

impl AccountHandle {
    pub const fn new(program_transformer: Arc<ProgramTransformer>) -> Self {
        Self(program_transformer)
    }
}

impl MessageHandler for AccountHandle {
    fn handle(
        &self,
        input: HashMap<String, RedisValue>,
    ) -> BoxFuture<'static, Result<(), IngestMessageError>> {
        let program_transformer = Arc::clone(&self.0);
        Box::pin(async move {
            let account = AccountInfo::try_parse_msg(input)?;
            program_transformer
                .handle_account_update(&account)
                .await
                .map_err(IngestMessageError::ProgramTransformer)
        })
    }
}

impl Clone for AccountHandle {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct TransactionHandle(Arc<ProgramTransformer>);

impl TransactionHandle {
    pub const fn new(program_transformer: Arc<ProgramTransformer>) -> Self {
        Self(program_transformer)
    }
}

impl MessageHandler for TransactionHandle {
    fn handle(
        &self,
        input: HashMap<String, RedisValue>,
    ) -> BoxFuture<'static, Result<(), IngestMessageError>> {
        let program_transformer = Arc::clone(&self.0);

        Box::pin(async move {
            let transaction = TransactionInfo::try_parse_msg(input)?;
            program_transformer
                .handle_transaction(&transaction)
                .await
                .map_err(IngestMessageError::ProgramTransformer)
        })
    }
}

impl Clone for TransactionHandle {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

#[derive(Clone)]
pub struct Acknowledge {
    config: Arc<ConfigIngestStream>,
    connection: MultiplexedConnection,
}

impl Acknowledge {
    pub const fn new(config: Arc<ConfigIngestStream>, connection: MultiplexedConnection) -> Self {
        Self { config, connection }
    }
}

impl Acknowledge {
    async fn handle(&self, ids: Vec<String>) {
        let mut connection = self.connection.clone();
        let config = &self.config;

        let count = ids.len();

        match redis::pipe()
            .atomic()
            .xack(&config.name, &config.group, &ids)
            .xdel(&config.name, &ids)
            .query_async::<_, redis::Value>(&mut connection)
            .await
        {
            Ok(response) => {
                debug!(
                    target: "acknowledge_handler",
                    "action=acknowledge_and_delete stream={} response={:?} expected={:?}",
                    config.name, response, count
                );

                redis_xack_inc(&config.name, &config.consumer, count);
            }
            Err(e) => {
                error!(
                    target: "acknowledge_handler",
                    "action=acknowledge_and_delete_failed stream={} error={:?}",
                    config.name, e
                );
            }
        }

        ack_tasks_total_dec(&config.name, &config.consumer);
    }
}

pub struct IngestStream<H: MessageHandler> {
    config: Arc<ConfigIngestStream>,
    connection: Option<MultiplexedConnection>,
    handler: Option<H>,
    _handler: PhantomData<H>,
}

impl<H: MessageHandler> IngestStream<H> {
    pub fn build() -> Self {
        Self {
            config: Arc::new(ConfigIngestStream::default()),
            connection: None,
            handler: None,
            _handler: PhantomData,
        }
    }

    pub fn handler(mut self, handler: H) -> Self {
        self.handler = Some(handler);
        self
    }

    pub fn config(mut self, config: ConfigIngestStream) -> Self {
        self.config = Arc::new(config);
        self
    }

    pub fn connection(mut self, connection: MultiplexedConnection) -> Self {
        self.connection = Some(connection);
        self
    }

    async fn read(&self, connection: &mut MultiplexedConnection) -> RedisResult<StreamReadReply> {
        let config = &self.config;

        let opts = StreamReadOptions::default()
            .group(&config.group, &config.consumer)
            .count(config.batch_size)
            .block(250);

        connection
            .xread_options(&[&config.name], &[">"], &opts)
            .await
    }

    pub async fn start(mut self) -> anyhow::Result<IngestStreamStop> {
        let config = Arc::clone(&self.config);
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

        let mut connection = self.connection.take().expect("Connection is required");
        let handler = self.handler.take().expect("Handler is required");

        xgroup_create(&mut connection, &config.name, &config.group).await?;

        xgroup_delete_consumer(
            &mut connection,
            &config.name,
            &config.group,
            &config.consumer,
        )
        .await?;

        xgroup_create_consumer(
            &mut connection,
            &config.name,
            &config.group,
            &config.consumer,
        )
        .await?;

        let (ack_tx, mut ack_rx) = tokio::sync::mpsc::channel::<String>(config.xack_buffer_size);
        let (ack_shutdown_tx, mut ack_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let (msg_tx, mut msg_rx) =
            tokio::sync::mpsc::channel::<Vec<StreamId>>(config.message_buffer_size);
        let (msg_shutdown_tx, mut msg_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let config_messages = Arc::clone(&config);

        let messages = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            let config = Arc::clone(&config_messages);
            let handler = handler.clone();

            loop {
                tokio::select! {
                    Some(ids) = msg_rx.recv() => {
                        for StreamId { id, map } in ids {
                            if tasks.len() >= config.max_concurrency {
                                tasks.join_next().await;
                            }

                            let handler = handler.clone();
                            let ack_tx = ack_tx.clone();
                            let config = Arc::clone(&config);

                            ingest_tasks_total_inc(&config.name, &config.consumer);

                            tasks.spawn(async move {
                                let start_time = tokio::time::Instant::now();
                                let result = handler.handle(map).await.map_err(IngestMessageError::into);
                                let elapsed_time = start_time.elapsed().as_secs_f64();

                                ingest_job_time_set(&config.name, &config.consumer, elapsed_time);

                                match result {
                                    Ok(()) => {
                                        program_transformer_task_status_inc(&config.name, &config.consumer, ProgramTransformerTaskStatusKind::Success);
                                    }
                                    Err(IngestMessageError::RedisStreamMessage(e)) => {
                                        error!("Failed to process message: {:?}", e);
                                        program_transformer_task_status_inc(&config.name, &config.consumer, e.into());
                                    }
                                    Err(IngestMessageError::DownloadMetadataJson(e)) => {
                                        program_transformer_task_status_inc(&config.name, &config.consumer, e.into());
                                    }
                                    Err(IngestMessageError::ProgramTransformer(e)) => {
                                        error!("Failed to process message: {:?}", e);
                                        program_transformer_task_status_inc(&config.name, &config.consumer, e.into());
                                    }
                                }

                                if let Err(e) = ack_tx.send(id).await {
                                    error!(target: "ingest_stream", "action=send_ack stream={} error={:?}", &config.name, e);
                                }

                                ingest_tasks_total_dec(&config.name, &config.consumer);
                            });
                        }
                    }
                    _ = &mut msg_shutdown_rx => {
                        break;
                    }
                }
            }

            while (tasks.join_next().await).is_some() {}
        });

        let ack = tokio::spawn({
            let config = Arc::clone(&config);
            let mut pending = Vec::new();
            let mut tasks = JoinSet::new();
            let handler = Arc::new(Acknowledge::new(Arc::clone(&config), connection.clone()));

            async move {
                let deadline = tokio::time::sleep(config.xack_batch_max_idle);
                tokio::pin!(deadline);

                loop {
                    tokio::select! {
                        Some(id) = ack_rx.recv() => {
                            pending.push(id);

                            if pending.len() >= config.xack_batch_max_size {
                                if tasks.len() >= config.ack_concurrency {
                                    tasks.join_next().await;
                                }

                                let ids = std::mem::take(&mut pending);
                                let handler = Arc::clone(&handler);


                                ack_tasks_total_inc(&config.name, &config.consumer);

                                tasks.spawn(async move {
                                    handler.handle(ids).await;
                                });

                                deadline.as_mut().reset(tokio::time::Instant::now() + config.xack_batch_max_idle);
                            }
                        }
                        _ = &mut deadline, if !pending.is_empty() => {
                            if tasks.len() >= config.ack_concurrency {
                                tasks.join_next().await;
                            }
                            let ids = std::mem::take(&mut pending);
                            let handler = Arc::clone(&handler);

                            ack_tasks_total_inc(&config.name, &config.consumer);

                            tasks.spawn(async move {
                                handler.handle(ids).await;
                            });

                            deadline.as_mut().reset(tokio::time::Instant::now() + config.xack_batch_max_idle);
                        }
                        _ = &mut ack_shutdown_rx => {
                            break;
                        }
                    }
                }

                if !pending.is_empty() {
                    let handler = Arc::clone(&handler);
                    handler.handle(std::mem::take(&mut pending)).await;
                }

                while (tasks.join_next().await).is_some() {}
            }
        });

        let labels = vec![config.name.clone()];
        tokio::spawn({
            let connection = connection.clone();
            let config = Arc::clone(&config);

            async move {
                let config = Arc::clone(&config);

                loop {
                    let connection = connection.clone();
                    let labels = labels.clone();

                    if let Err(e) = report_xlen(connection, labels).await {
                        error!(target: "ingest_stream", "action=report_xlen stream={} error={:?}", &config.name, e);
                    }

                    sleep(Duration::from_millis(100)).await;
                }
            }
        });

        let control = tokio::spawn({
            let mut connection = connection.clone();

            async move {
                let config = Arc::clone(&config);

                debug!(target: "ingest_stream", "action=read_stream_start stream={}", config.name);

                loop {
                    let config = Arc::clone(&config);

                    tokio::select! {
                        _ = &mut shutdown_rx => {
                            if let Err(e) = msg_shutdown_tx.send(()) {
                                error!(target: "ingest_stream", "action=msg_shutdown stream={} error={:?}", &config.name, e);
                            }

                            if let Err(e) = messages.await {
                                error!(target: "ingest_stream", "action=await_messages stream={} error={:?}", &config.name, e);
                            }

                            if let Err(e) = ack_shutdown_tx.send(()) {
                                error!(target: "ingest_stream", "action=ack_shutdown stream={} error={:?}", &config.name, e);
                            }

                            if let Err(e) = ack.await {
                                error!(target: "ingest_stream", "action=ack_shutdown stream={} error={:?}", &config.name, e);
                            }

                            break;
                        },
                        result = self.read(&mut connection) => {
                            match result {
                                Ok(reply) => {
                                    for StreamKey { key: _, ids } in reply.keys {
                                        let config = Arc::clone(&config);
                                        let count = ids.len();
                                        debug!(target: "ingest_stream", "action=xread stream={} count={:?}", &config.name, count);

                                        redis_xread_inc(&config.name, &config.consumer, count);

                                        if let Err(e) = msg_tx.send(ids).await {
                                            error!(target: "ingest_stream", "action=send_ids stream={} error={:?}", &config.name, e);
                                        }
                                    }
                                }
                                Err(err) => {
                                    error!(target: "ingest_stream", "action=xread stream={} error={:?}", &config.name, err);
                                }
                            }
                        }
                    }
                }

                warn!(target: "ingest_stream", "action=stream_shutdown stream={} stream shutdown", config.name);
            }
        });

        Ok(IngestStreamStop {
            control,
            shutdown_tx,
        })
    }
}

#[derive(Clone)]
pub struct TrackedPipeline {
    pipeline: redis::Pipeline,
    count: usize,
}

impl Default for TrackedPipeline {
    fn default() -> Self {
        Self {
            pipeline: redis::pipe(),
            count: 0,
        }
    }
}

impl TrackedPipeline {
    pub fn xadd_maxlen<F, V>(&mut self, key: &str, maxlen: StreamMaxlen, id: F, field: V)
    where
        F: redis::ToRedisArgs,
        V: redis::ToRedisArgs,
    {
        self.pipeline
            .xadd_maxlen(key, maxlen, id, &[(REDIS_STREAM_DATA_KEY, field)]);
        self.count += 1;
    }

    pub async fn flush(&mut self, connection: &mut MultiplexedConnection) -> Result<usize, usize> {
        let result: RedisResult<RedisValue> = self.pipeline.atomic().query_async(connection).await;
        let count = self.count;
        self.count = 0;
        self.pipeline.clear();

        match result {
            Ok(_) => Ok(count),
            Err(_) => Err(count),
        }
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

    Ok(())
}

pub async fn xgroup_create_consumer<C: AsyncCommands>(
    connection: &mut C,
    name: &str,
    group: &str,
    consumer: &str,
) -> anyhow::Result<()> {
    let result: RedisResult<RedisValue> = redis::cmd("XGROUP")
        .arg("CREATECONSUMER")
        .arg(name)
        .arg(group)
        .arg(consumer)
        .query_async(connection)
        .await;

    match result {
        Ok(_) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub async fn xgroup_delete_consumer<C: AsyncCommands>(
    connection: &mut C,
    name: &str,
    group: &str,
    consumer: &str,
) -> anyhow::Result<()> {
    let result: RedisResult<RedisValue> = redis::cmd("XGROUP")
        .arg("DELCONSUMER")
        .arg(name)
        .arg(group)
        .arg(consumer)
        .query_async(connection)
        .await;

    match result {
        Ok(_) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
