use {
    anyhow::Context,
    serde::{de, Deserialize},
    std::{collections::HashMap, net::SocketAddr, path::Path, time::Duration},
    tokio::fs,
    yellowstone_grpc_tools::config::{
        deserialize_usize_str, ConfigGrpcRequestAccounts, ConfigGrpcRequestCommitment,
        ConfigGrpcRequestTransactions,
    },
};

pub const REDIS_STREAM_DATA_KEY: &str = "data";

pub async fn load<T>(path: impl AsRef<Path> + Copy) -> anyhow::Result<T>
where
    T: de::DeserializeOwned,
{
    let text = fs::read_to_string(path)
        .await
        .context("failed to read config from file")?;

    match path.as_ref().extension().and_then(|e| e.to_str()) {
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&text).context("failed to parse config from YAML file")
        }
        Some("json") => json5::from_str(&text).context("failed to parse config from JSON file"),
        value => anyhow::bail!("unknown config extension: {value:?}"),
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfigIngestStream {
    pub name: String,
    #[serde(default = "ConfigIngestStream::default_group")]
    pub group: String,
    #[serde(default = "ConfigIngestStream::default_consumer")]
    pub consumer: String,
    #[serde(
        default = "ConfigIngestStream::default_xack_batch_max_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub xack_batch_max_size: usize,
    #[serde(
        default = "ConfigIngestStream::default_xack_batch_max_idle",
        deserialize_with = "deserialize_duration_str",
        rename = "xack_batch_max_idle_ms"
    )]
    pub xack_batch_max_idle: Duration,
    #[serde(
        default = "ConfigIngestStream::default_batch_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub batch_size: usize,
    #[serde(
        default = "ConfigIngestStream::default_max_concurrency",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_concurrency: usize,
    #[serde(
        default = "ConfigIngestStream::default_ack_concurrency",
        deserialize_with = "deserialize_usize_str"
    )]
    pub ack_concurrency: usize,
    #[serde(
        default = "ConfigIngestStream::default_xack_buffer_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub xack_buffer_size: usize,
    #[serde(
        default = "ConfigIngestStream::default_message_buffer_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub message_buffer_size: usize,
}

impl ConfigIngestStream {
    pub const fn default_xack_buffer_size() -> usize {
        1_000
    }

    pub const fn default_message_buffer_size() -> usize {
        100
    }

    pub const fn default_max_concurrency() -> usize {
        2
    }

    pub const fn default_ack_concurrency() -> usize {
        5
    }

    pub const fn default_xack_batch_max_idle() -> Duration {
        Duration::from_millis(10_000)
    }

    pub fn default_group() -> String {
        "ingester".to_owned()
    }

    pub fn default_consumer() -> String {
        "consumer".to_owned()
    }

    pub const fn default_xack_batch_max_size() -> usize {
        5
    }

    pub const fn default_batch_size() -> usize {
        5
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ConfigPrometheus {
    pub prometheus: Option<SocketAddr>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfigGeyser {
    pub endpoint: String,
    pub x_token: Option<String>,
    #[serde(default = "ConfigGeyser::default_commitment")]
    pub commitment: ConfigGrpcRequestCommitment,
    #[serde(default = "ConfigGeyser::default_connection_timeout")]
    pub connect_timeout: u64,
    #[serde(default = "ConfigGeyser::default_timeout")]
    pub timeout: u64,
}

impl ConfigGeyser {
    pub const fn default_commitment() -> ConfigGrpcRequestCommitment {
        ConfigGrpcRequestCommitment::Finalized
    }

    pub const fn default_connection_timeout() -> u64 {
        10
    }

    pub const fn default_timeout() -> u64 {
        10
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfigStream {
    pub name: String,
    #[serde(
        default = "ConfigStream::default_stream_maxlen",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_len: usize,
    #[serde(
        default = "ConfigStream::default_max_concurrency",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_concurrency: usize,
}

impl ConfigStream {
    pub const fn default_stream_maxlen() -> usize {
        10_000_000
    }

    pub const fn default_max_concurrency() -> usize {
        10
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfigGrpcRequestFilter {
    pub accounts: Option<ConfigGrpcRequestAccounts>,
    pub transactions: Option<ConfigGrpcRequestTransactions>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfigSubscription {
    pub stream: ConfigStream,
    pub filter: ConfigGrpcRequestFilter,
}

pub type ConfigGrpcSubscriptions = HashMap<String, ConfigSubscription>;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfigGrpc {
    pub geyser: ConfigGeyser,

    pub subscriptions: ConfigGrpcSubscriptions,

    pub redis: ConfigGrpcRedis,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConfigGrpcRedis {
    pub url: String,
    #[serde(
        default = "ConfigGrpcRedis::default_pipeline_max_idle",
        deserialize_with = "deserialize_duration_str"
    )]
    pub pipeline_max_idle: Duration,
}

impl ConfigGrpcRedis {
    pub const fn default_pipeline_max_idle() -> Duration {
        Duration::from_millis(10)
    }
}

pub fn deserialize_duration_str<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: de::Deserializer<'de>,
{
    let ms = deserialize_usize_str(deserializer)?;
    Ok(Duration::from_millis(ms as u64))
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigIngester {
    pub redis: String,
    pub postgres: ConfigPostgres,
    pub download_metadata: ConfigIngesterDownloadMetadata,
    pub snapshots: ConfigIngestStream,
    pub accounts: ConfigIngestStream,
    pub transactions: ConfigIngestStream,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigIngesterRedisStreamType {
    Account,
    Transaction,
    MetadataJson,
    Snapshot,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigPostgres {
    pub url: String,
    #[serde(
        default = "ConfigPostgres::default_min_connections",
        deserialize_with = "deserialize_usize_str"
    )]
    pub min_connections: usize,
    #[serde(
        default = "ConfigPostgres::default_max_connections",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_connections: usize,
    #[serde(
        default = "ConfigPostgres::default_idle_timeout",
        deserialize_with = "deserialize_duration_str"
    )]
    pub idle_timeout: Duration,
    #[serde(
        default = "ConfigPostgres::default_max_lifetime",
        deserialize_with = "deserialize_duration_str"
    )]
    pub max_lifetime: Duration,
}

impl ConfigPostgres {
    pub const fn default_min_connections() -> usize {
        10
    }

    pub const fn default_max_connections() -> usize {
        50
    }

    pub const fn default_idle_timeout() -> Duration {
        Duration::from_millis(75)
    }

    pub const fn default_max_lifetime() -> Duration {
        Duration::from_millis(125)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigIngesterDownloadMetadata {
    pub stream: ConfigIngestStream,
    #[serde(
        default = "ConfigIngesterDownloadMetadata::default_num_threads",
        deserialize_with = "deserialize_usize_str"
    )]
    pub _num_threads: usize,
    #[serde(
        default = "ConfigIngesterDownloadMetadata::default_max_attempts",
        deserialize_with = "deserialize_usize_str"
    )]
    pub _max_attempts: usize,
    #[serde(
        default = "ConfigIngesterDownloadMetadata::default_request_timeout",
        deserialize_with = "deserialize_duration_str",
        rename = "request_timeout_ms"
    )]
    pub request_timeout: Duration,
    #[serde(
        default = "ConfigIngesterDownloadMetadata::default_stream_maxlen",
        deserialize_with = "deserialize_usize_str"
    )]
    pub stream_maxlen: usize,
    #[serde(
        default = "ConfigIngesterDownloadMetadata::default_stream_max_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub _pipeline_max_size: usize,
    #[serde(
        default = "ConfigIngesterDownloadMetadata::default_pipeline_max_idle",
        deserialize_with = "deserialize_duration_str",
        rename = "pipeline_max_idle_ms"
    )]
    pub _pipeline_max_idle: Duration,
}

impl ConfigIngesterDownloadMetadata {
    pub const fn default_num_threads() -> usize {
        2
    }

    pub const fn default_pipeline_max_idle() -> Duration {
        Duration::from_millis(10)
    }

    pub const fn default_stream_max_size() -> usize {
        10
    }

    pub const fn default_stream_maxlen() -> usize {
        10_000_000
    }

    pub const fn default_max_attempts() -> usize {
        3
    }

    pub const fn default_request_timeout() -> Duration {
        Duration::from_millis(3_000)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigMonitor {
    pub postgres: ConfigPostgres,
    pub rpc: String,
    pub bubblegum: ConfigBubblegumVerify,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigBubblegumVerify {
    #[serde(
        default = "ConfigBubblegumVerify::default_report_interval",
        deserialize_with = "deserialize_duration_str"
    )]
    pub _report_interval: Duration,
    #[serde(default)]
    pub only_trees: Option<Vec<String>>,
    #[serde(
        default = "ConfigBubblegumVerify::default_max_concurrency",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_concurrency: usize,
}

impl ConfigBubblegumVerify {
    pub const fn default_report_interval() -> Duration {
        Duration::from_millis(5 * 60 * 1000)
    }
    pub const fn default_max_concurrency() -> usize {
        20
    }
}
