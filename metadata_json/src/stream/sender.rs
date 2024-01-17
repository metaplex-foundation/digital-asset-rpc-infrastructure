use super::METADATA_JSON_STREAM;
use anyhow::Result;
use clap::Parser;
use figment::value::{Dict, Value};
use plerkle_messenger::{Messenger, MessengerConfig, MessengerType};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::Deserialize;
use std::num::TryFromIntError;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::{mpsc::error::TrySendError, Mutex};

#[derive(Clone, Debug, Parser, Deserialize, PartialEq)]
pub struct SenderArgs {
    #[arg(long, env)]
    pub messenger_redis_url: String,
    #[arg(long, env, default_value = "100")]
    pub messenger_redis_batch_size: String,
    #[arg(long, env, default_value = "5")]
    pub messenger_queue_connections: u64,
}

fn rand_string() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(30)
        .map(char::from)
        .collect()
}

impl From<SenderArgs> for MessengerConfig {
    fn from(args: SenderArgs) -> Self {
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
        connection_config.insert("consumer_id".to_string(), Value::from(rand_string()));

        Self {
            messenger_type: MessengerType::Redis,
            connection_config,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SenderPoolError {
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
pub struct SenderPool {
    tx: mpsc::Sender<Box<dyn plerkle_messenger::Messenger>>,
    rx: Arc<Mutex<mpsc::Receiver<Box<dyn plerkle_messenger::Messenger>>>>,
}

impl SenderPool {
    #[allow(dead_code)]
    pub async fn try_from_config(config: SenderArgs) -> anyhow::Result<Self, SenderPoolError> {
        let size = usize::try_from(config.messenger_queue_connections)?;
        let (tx, rx) = mpsc::channel(size);

        for _ in 0..config.messenger_queue_connections {
            let messenger_config: MessengerConfig = config.clone().into();
            let mut messenger = plerkle_messenger::select_messenger(messenger_config).await?;
            messenger.add_stream(METADATA_JSON_STREAM).await?;
            messenger
                .set_buffer_size(METADATA_JSON_STREAM, 10000000000000000)
                .await;

            tx.try_send(messenger)?;
        }

        Ok(Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        })
    }
    #[allow(dead_code)]
    pub async fn push(&self, message: &[u8]) -> Result<(), SenderPoolError> {
        let mut rx = self.rx.lock().await;
        let mut messenger = rx
            .recv()
            .await
            .ok_or(SenderPoolError::RecvMessengerConnection)?;

        messenger.send(METADATA_JSON_STREAM, message).await?;

        self.tx.send(messenger).await?;

        Ok(())
    }
}
