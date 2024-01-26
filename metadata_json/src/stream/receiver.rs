use super::METADATA_JSON_STREAM;
use clap::Parser;
use figment::value::{Dict, Value};
use plerkle_messenger::{select_messenger, Messenger, MessengerConfig, MessengerType, RecvData};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Parser)]
pub struct ReceiverArgs {
    #[arg(long, env)]
    pub messenger_redis_url: String,
    #[arg(long, env, default_value = "100")]
    pub messenger_redis_batch_size: String,
}

fn rand_string() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(30)
        .map(char::from)
        .collect()
}

impl From<ReceiverArgs> for MessengerConfig {
    fn from(args: ReceiverArgs) -> Self {
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
pub enum ReceiverError {
    #[error("messenger: {0}")]
    Messenger(#[from] plerkle_messenger::MessengerError),
}

#[derive(Clone)]
pub struct Receiver(Arc<Mutex<Box<dyn Messenger>>>);

impl Receiver {
    pub async fn try_from_config(config: MessengerConfig) -> Result<Self, anyhow::Error> {
        let mut messenger = select_messenger(config).await?;

        messenger.add_stream(METADATA_JSON_STREAM).await?;
        messenger
            .set_buffer_size(METADATA_JSON_STREAM, 10000000000000000)
            .await;

        Ok(Self(Arc::new(Mutex::new(messenger))))
    }

    pub async fn recv(&self) -> Result<Vec<RecvData>, ReceiverError> {
        let mut messenger = self.0.lock().await;

        messenger
            .recv(
                METADATA_JSON_STREAM,
                plerkle_messenger::ConsumptionType::All,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn ack(&self, ids: &[String]) -> Result<(), ReceiverError> {
        let mut messenger = self.0.lock().await;

        messenger
            .ack_msg(METADATA_JSON_STREAM, ids)
            .await
            .map_err(Into::into)
    }
}
