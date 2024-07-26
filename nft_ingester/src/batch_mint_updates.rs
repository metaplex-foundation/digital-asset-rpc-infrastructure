use crate::error::IngesterError;
use async_channel::Receiver;
use log::error;
use sqlx::postgres::PgListener;
use tokio::task::{JoinError, JoinSet};

const BATCH_MINT_LISTEN_KEY: &str = "new_batch_mint";

pub async fn create_batch_mint_notification_channel(
    database_url: &str,
    tasks: &mut JoinSet<Result<(), JoinError>>,
) -> Result<Receiver<()>, IngesterError> {
    let mut listener = PgListener::connect(database_url).await?;
    listener.listen(BATCH_MINT_LISTEN_KEY).await?;
    let (s, r) = async_channel::unbounded::<()>();
    tasks.spawn(async move {
        loop {
            if let Err(e) = listener.recv().await {
                error!("Recv batch mint notification: {}", e);
                continue;
            }
            if let Err(e) = s.send(()).await {
                error!("Send batch mint notification: {}", e);
            }
        }
    });

    Ok(r)
}
