use {
    crate::{
        config::{ConfigIngesterRedis, ConfigIngesterRedisStreamType},
        prom::{redis_xack_inc, redis_xlen_set},
    },
    futures::future::{BoxFuture, FutureExt},
    redis::{
        aio::MultiplexedConnection,
        streams::{
            StreamClaimReply, StreamId, StreamKey, StreamPendingCountReply, StreamReadOptions,
            StreamReadReply,
        },
        AsyncCommands, ErrorKind as RedisErrorKind, RedisResult, Value as RedisValue,
    },
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
    yellowstone_grpc_proto::{
        prelude::{SubscribeUpdateAccount, SubscribeUpdateTransaction},
        prost::Message,
    },
};

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
    stream_data_key: String,
    xack_batch_max_size: usize,
    xack_batch_max_idle: Duration,
    xack_max_in_process: usize,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum RedisStreamMessage {
    Account(SubscribeUpdateAccount),
    Transaction(SubscribeUpdateTransaction),
}

#[derive(Debug)]
pub struct RedisStreamMessageInfo {
    id: String,
    data: RedisStreamMessage,
    ack_tx: mpsc::UnboundedSender<String>,
}

impl RedisStreamMessageInfo {
    fn parse(
        stream: &RedisStreamInfo,
        StreamId { id, map }: StreamId,
        ack_tx: mpsc::UnboundedSender<String>,
    ) -> anyhow::Result<Self> {
        let data = match map.get(&stream.stream_data_key) {
            Some(RedisValue::Data(vec)) => match stream.stream_type {
                ConfigIngesterRedisStreamType::Account => {
                    RedisStreamMessage::Account(SubscribeUpdateAccount::decode(vec.as_ref())?)
                }
                ConfigIngesterRedisStreamType::Transaction => RedisStreamMessage::Transaction(
                    SubscribeUpdateTransaction::decode(vec.as_ref())?,
                ),
            },
            Some(_) => anyhow::bail!(
                "invalid data (key: {:?}) from stream {:?}",
                stream.stream_data_key,
                stream.stream_name
            ),
            None => anyhow::bail!(
                "failed to get data (key: {:?}) from stream {:?}",
                stream.stream_data_key,
                stream.stream_name
            ),
        };
        Ok(Self { id, data, ack_tx })
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

impl RedisStream {
    pub async fn new(
        config: ConfigIngesterRedis,
        mut connection: MultiplexedConnection,
    ) -> anyhow::Result<(Self, BoxFuture<'static, anyhow::Result<()>>)> {
        // create group with consumer per stream
        for stream in config.streams.iter() {
            xgroup_create(
                &mut connection,
                &stream.stream,
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
                    stream_name: stream.stream.clone(),
                    stream_type: stream.stream_type,
                    stream_data_key: stream.data_key.clone(),
                    xack_batch_max_size: stream.xack_batch_max_size,
                    xack_batch_max_idle: stream.xack_batch_max_idle,
                    xack_max_in_process: stream.xack_max_in_process,
                });
                ack_tasks.push((Arc::clone(&info), ack_rx));
                (stream.stream.clone(), (ack_tx, info))
            })
            .collect::<HashMap<_, _>>();

        // spawn xack tasks
        let mut tasks = ack_tasks
            .into_iter()
            .map(|(stream, ack_rx)| {
                let connection = connection.clone();
                tokio::spawn(async move { Self::run_ack(stream, connection, ack_rx).await })
            })
            .collect::<Vec<_>>();

        // spawn prefetch task
        let (messages_tx, messages_rx) = mpsc::channel(config.prefetch_queue_size);
        tasks.push(tokio::spawn({
            let shutdown = Arc::clone(&shutdown);
            async move { Self::run_prefetch(config, streams, connection, messages_tx, shutdown).await }
        }));

        // merge spawned xack / prefetch tasks
        let spawned_tasks = async move {
            for task in tasks.into_iter() {
                task.await??;
            }
            Ok::<(), anyhow::Error>(())
        }
        .boxed();

        Ok((
            Self {
                shutdown,
                messages_rx,
            },
            spawned_tasks,
        ))
    }

    pub async fn recv(&mut self) -> Option<RedisStreamMessageInfo> {
        self.messages_rx.recv().await
    }

    pub fn shutdown(self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    async fn run_prefetch(
        config: ConfigIngesterRedis,
        streams: HashMap<String, (mpsc::UnboundedSender<String>, Arc<RedisStreamInfo>)>,
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

                // read pending keys
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

        let streams_keys = streams.keys().map(|name| name.as_str()).collect::<Vec<_>>();
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
                let (ack_tx, stream) = match streams.get(&key) {
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
                        redis::pipe()
                            .atomic()
                            .xack(&stream.stream_name, &stream.group, &ids)
                            .xdel(&stream.stream_name, &ids)
                            .query_async(&mut connection)
                            .await?;
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
