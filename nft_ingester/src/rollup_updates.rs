use crate::error::IngesterError;
use async_channel::Receiver;
use log::error;
use sqlx::postgres::PgListener;
use std::time::Duration;
use tokio::task::{JoinError, JoinSet};

const ROLLUP_LISTEN_KEY: &str = "new_rollup";

pub async fn create_rollup_notification_channel(
    database_url: &str,
    tasks: &mut JoinSet<Result<(), JoinError>>,
) -> Result<Receiver<()>, IngesterError> {
    let mut listener = PgListener::connect(database_url).await?;
    listener.listen(ROLLUP_LISTEN_KEY).await?;
    let (s, r) = async_channel::unbounded::<()>();
    tasks.spawn(async move {
        loop {
            if let Err(e) = listener.recv().await {
                error!("Recv rollup notification: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            if let Err(e) = s.send(()).await {
                error!("Send rollup notification: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    });

    Ok(r)
}
