use anyhow::Result;
use clap::Parser;
use figment::value::{Dict, Value};
use plerkle_messenger::{
    Messenger, MessengerConfig, MessengerType, ACCOUNT_BACKFILL_STREAM, ACCOUNT_STREAM,
    TRANSACTION_BACKFILL_STREAM, TRANSACTION_STREAM,
};
use std::num::TryFromIntError;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::{mpsc::error::TrySendError, Mutex};

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
            connection_config,
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

            let streams = [
                (plerkle_messenger::ACCOUNT_STREAM, 100_000_000),
                (plerkle_messenger::ACCOUNT_BACKFILL_STREAM, 100_000_000),
                (plerkle_messenger::SLOT_STREAM, 100_000),
                (plerkle_messenger::TRANSACTION_STREAM, 10_000_000),
                (plerkle_messenger::TRANSACTION_BACKFILL_STREAM, 10_000_000),
                (plerkle_messenger::BLOCK_STREAM, 100_000),
            ];

            for &(key, size) in &streams {
                messenger.add_stream(key).await?;
                messenger.set_buffer_size(key, size).await;
            }

            tx.try_send(messenger)?;
        }

        Ok(Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        })
    }

    /// Pushes account backfill data to the appropriate stream.
    ///
    /// This method sends account backfill data to the `ACCOUNT_BACKFILL_STREAM`.
    /// It is used for backfilling account information in the system.
    ///
    /// # Arguments
    ///
    /// * `bytes` - A byte slice containing the account backfill data to be pushed.
    ///
    /// # Returns
    ///
    /// This method returns a `Result` which is `Ok` if the push is successful,
    /// or an `Err` with a `QueuePoolError` if the push fails.
    pub async fn push_account_backfill(&self, bytes: &[u8]) -> Result<(), QueuePoolError> {
        self.push(ACCOUNT_BACKFILL_STREAM, bytes).await
    }

    /// Pushes transaction backfill data to the appropriate stream.
    ///
    /// This method sends transaction backfill data to the `TRANSACTION_BACKFILL_STREAM`.
    /// It is used for backfilling transaction information in the system.
    ///
    /// # Arguments
    ///
    /// * `bytes` - A byte slice containing the transaction backfill data to be pushed.
    ///
    /// # Returns
    ///
    /// This method returns a `Result` which is `Ok` if the push is successful,
    /// or an `Err` with a `QueuePoolError` if the push fails.
    pub async fn push_transaction_backfill(&self, bytes: &[u8]) -> Result<(), QueuePoolError> {
        self.push(TRANSACTION_BACKFILL_STREAM, bytes).await
    }

    /// Pushes account data to the appropriate stream.
    ///
    /// This method sends account data to the `ACCOUNT_STREAM`.
    /// It is used for pushing real-time account updates to the system.
    ///
    /// # Arguments
    ///
    /// * `bytes` - A byte slice containing the account data to be pushed.
    ///
    /// # Returns
    ///
    /// This method returns a `Result` which is `Ok` if the push is successful,
    /// or an `Err` with a `QueuePoolError` if the push fails.
    pub async fn push_account(&self, bytes: &[u8]) -> Result<(), QueuePoolError> {
        self.push(ACCOUNT_STREAM, bytes).await
    }

    /// Pushes transaction data to the appropriate stream.
    ///
    /// This method sends transaction data to the `TRANSACTION_STREAM`.
    /// It is used for pushing real-time transaction updates to the system.
    ///
    /// # Arguments
    ///
    /// * `bytes` - A byte slice containing the transaction data to be pushed.
    ///
    /// # Returns
    ///
    /// This method returns a `Result` which is `Ok` if the push is successful,
    /// or an `Err` with a `QueuePoolError` if the push fails.
    pub async fn push_transaction(&self, bytes: &[u8]) -> Result<(), QueuePoolError> {
        self.push(TRANSACTION_STREAM, bytes).await
    }

    async fn push(&self, stream_key: &'static str, bytes: &[u8]) -> Result<(), QueuePoolError> {
        let mut rx = self.rx.lock().await;
        let mut messenger = rx
            .recv()
            .await
            .ok_or(QueuePoolError::RecvMessengerConnection)?;

        messenger.send(stream_key, bytes).await?;

        self.tx.send(messenger).await?;

        Ok(())
    }
}
