use anyhow::Result;
use clap::Parser;
use figment::value::{Dict, Value};
use plerkle_messenger::{
    redis_messenger::RedisMessenger, Messenger, MessengerConfig, MessengerError, MessengerType,
};

const TRANSACTION_BACKFILL_STREAM: &str = "TXNFILL";

#[derive(Clone, Debug, Parser)]
pub struct QueueArgs {
    #[arg(long, env)]
    pub messenger_redis_url: String,
    #[arg(long, env, default_value = "100")]
    pub messenger_redis_batch_size: String,
    #[arg(long, env, default_value = "10000000")]
    pub messenger_stream_max_buffer_size: usize,
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

        Self {
            messenger_type: MessengerType::Redis,
            connection_config,
        }
    }
}

#[derive(Debug)]
pub struct Queue(RedisMessenger);

impl Queue {
    pub async fn setup(config: QueueArgs) -> Result<Self, MessengerError> {
        let mut messenger = RedisMessenger::new(config.clone().into()).await?;

        messenger.add_stream(TRANSACTION_BACKFILL_STREAM).await?;

        messenger
            .set_buffer_size(
                TRANSACTION_BACKFILL_STREAM,
                config.messenger_stream_max_buffer_size,
            )
            .await;

        Ok(Self(messenger))
    }

    pub async fn push(&mut self, message: &[u8]) -> Result<(), MessengerError> {
        self.0.send(TRANSACTION_BACKFILL_STREAM, message).await
    }
}
