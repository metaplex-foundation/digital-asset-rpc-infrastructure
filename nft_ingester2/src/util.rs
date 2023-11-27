use {
    futures::future::{BoxFuture, FutureExt},
    tokio::signal::unix::{signal, SignalKind},
};

pub fn create_shutdown() -> anyhow::Result<BoxFuture<'static, SignalKind>> {
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    Ok(async move {
        tokio::select! {
            _ = sigint.recv() => SignalKind::interrupt(),
            _ = sigterm.recv() => SignalKind::terminate(),
        }
    }
    .boxed())
}
