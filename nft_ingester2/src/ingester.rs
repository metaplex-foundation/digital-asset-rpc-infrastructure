use {
    crate::{
        config::ConfigIngester,
        redis::{metrics_xlen, RedisStream},
        util::create_shutdown,
    },
    futures::future::{Fuse, FusedFuture, FutureExt},
    tokio::signal::unix::SignalKind,
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

    // create redis stream reader
    let (mut redis_messages, redis_tasks) = RedisStream::new(config.redis, connection).await?;
    let redis_tasks_fut = Fuse::terminated();
    tokio::pin!(redis_tasks_fut);
    redis_tasks_fut.set(redis_tasks.fuse());

    // read messages in the loop
    let mut shutdown = create_shutdown()?;
    let result = loop {
        tokio::select! {
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
                tracing::warn!("{signal} received, waiting spawned tasks...");
                break Ok(());
            },
            result = &mut redis_tasks_fut => break result,
            msg = redis_messages.recv() => match msg {
                Some(msg) => {
                    // TODO: process messages here
                    msg.ack()?;
                }
                None => break Ok(()),
            }
        };
    };

    redis_messages.shutdown();
    if !redis_tasks_fut.is_terminated() {
        redis_tasks_fut.await?;
    }

    result
}
