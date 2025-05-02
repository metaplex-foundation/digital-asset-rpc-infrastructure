use {
    async_stream::stream,
    futures::stream::{BoxStream, StreamExt},
    tokio::signal::unix::{signal, SignalKind},
};

pub fn create_shutdown() -> anyhow::Result<BoxStream<'static, &'static str>> {
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    Ok(stream! {
        loop {
            yield tokio::select! {
                _ = sigint.recv() => "SIGINT",
                _ = sigterm.recv() => "SIGTERM",
            };
        }
    }
    .boxed())
}
