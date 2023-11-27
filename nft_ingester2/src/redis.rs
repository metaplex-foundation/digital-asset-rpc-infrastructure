use {
    crate::prom::redis_stream_len_set,
    redis::AsyncCommands,
    std::convert::Infallible,
    tokio::time::{sleep, Duration},
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
            redis_stream_len_set(stream, xlen);
        }

        sleep(Duration::from_millis(100)).await;
    }
}
