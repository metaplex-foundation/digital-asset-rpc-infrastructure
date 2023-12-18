use anyhow::Result;
use clap::Parser;
use figment::value::{Dict, Value};
use plerkle_messenger::{Messenger, MessengerConfig, MessengerType};
use std::num::TryFromIntError;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::{mpsc::error::TrySendError, Mutex};

const TRANSACTION_BACKFILL_STREAM: &'static str = "TXNFILL";

#[derive(Clone, Debug, Parser)]
pub struct QueueArgs {
    #[arg(long, env)]
    pub messenger_redis_url: String,
    #[arg(long, env, default_value = "100")]
    pub messenger_redis_batch_size: String,
    #[arg(long, env, default_value = "25")]
    pub messenger_queue_connections: u64,
}

impl From<QueueArgs> for MessengerConfig {
    fn from(args: QueueArgs) -> Self {
        let mut connection_config = Dict::new();

        connection_config.insert(
            "redis_connection_str".to_string(),
            Value::from(args.messenger_redis_url),
        );
        connection_config.insert(
            "batch_size".to_string(),
            Value::from(args.messenger_redis_batch_size),
        );
        connection_config.insert(
            "pipeline_size_bytes".to_string(),
            Value::from(1u128.to_string()),
        );

        Self {
            messenger_type: MessengerType::Redis,
            connection_config: connection_config,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum QueuePoolError {
    #[error("messenger")]
    Messenger(#[from] plerkle_messenger::MessengerError),
    #[error("tokio try send to channel")]
    TrySendMessengerChannel(#[from] TrySendError<Box<dyn Messenger>>),
    #[error("revc messenger connection")]
    RecvMessengerConnection,
    #[error("try from int")]
    TryFromInt(#[from] TryFromIntError),
    #[error("tokio send to channel")]
    SendMessengerChannel(#[from] mpsc::error::SendError<Box<dyn Messenger>>),
}

#[derive(Debug, Clone)]
pub struct QueuePool {
    tx: mpsc::Sender<Box<dyn plerkle_messenger::Messenger>>,
    rx: Arc<Mutex<mpsc::Receiver<Box<dyn plerkle_messenger::Messenger>>>>,
}

impl QueuePool {
    pub async fn try_from_config(config: QueueArgs) -> anyhow::Result<Self, QueuePoolError> {
        let size = usize::try_from(config.messenger_queue_connections)?;
        let (tx, rx) = mpsc::channel(size);

        for _ in 0..config.messenger_queue_connections {
            let messenger_config: MessengerConfig = config.clone().into();
            let mut messenger = plerkle_messenger::select_messenger(messenger_config).await?;
            messenger.add_stream(TRANSACTION_BACKFILL_STREAM).await?;
            messenger
                .set_buffer_size(TRANSACTION_BACKFILL_STREAM, 10000000000000000)
                .await;

            tx.try_send(messenger)?;
        }

        Ok(Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        })
    }

    pub async fn push(&self, message: &[u8]) -> Result<(), QueuePoolError> {
        let mut rx = self.rx.lock().await;
        let mut messenger = rx
            .recv()
            .await
            .ok_or(QueuePoolError::RecvMessengerConnection)?;

        messenger.send(TRANSACTION_BACKFILL_STREAM, message).await?;

        self.tx.send(messenger).await?;

        Ok(())
    }
}
