use std::time::Duration;
use log::error;
use sqlx::postgres::PgListener;
use tokio::task::{JoinError, JoinSet};

const ROLLUP_LISTEN_KEY: &str = "new_rollup";

pub(crate) async fn create_rollup_notification_channel(database_url: &str, tasks: &mut JoinSet<Result<(), JoinError>>) {
    let mut listener = match PgListener::connect(database_url).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("New rollup listener: {}", e);
            return;
        }
    };
    if let Err(e) = listener.listen(ROLLUP_LISTEN_KEY).await {
        error!("New rollup listener: {}", e);
        return;
    };

    let (s, r) = async_channel::unbounded::<()>();

    tasks.spawn(async move {
        loop {
            if let Err(e) = listener.recv().await {
                error!("Recv rollup notification: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
        }
            s.send(()).await
    });
}
