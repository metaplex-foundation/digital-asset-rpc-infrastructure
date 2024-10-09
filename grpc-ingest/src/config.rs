use {
    anyhow::Context,
    serde::{de, Deserialize},
    std::{net::SocketAddr, path::Path, time::Duration},
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
        default = "ConfigIngestStream::default_xack_buffer_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub xack_buffer_size: usize,
}

impl ConfigIngestStream {
    pub const fn default_xack_buffer_size() -> usize {
        1_000
    }
    pub const fn default_max_concurrency() -> usize {
        2
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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ConfigTopograph {
    #[serde(default = "ConfigTopograph::default_num_threads")]
    pub num_threads: usize,
}

impl ConfigTopograph {
    pub const fn default_num_threads() -> usize {
        5
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ConfigPrometheus {
    pub prometheus: Option<SocketAddr>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigGrpc {
    pub x_token: Option<String>,

    pub commitment: ConfigGrpcRequestCommitment,
    pub accounts: ConfigGrpcAccounts,
    pub transactions: ConfigGrpcTransactions,

    pub geyser_endpoint: String,

    pub redis: ConfigGrpcRedis,

    #[serde(
        default = "ConfigGrpc::default_max_concurrency",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_concurrency: usize,
}

impl ConfigGrpc {
    pub const fn default_max_concurrency() -> usize {
        10
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigGrpcAccounts {
    pub stream: String,
    #[serde(
        default = "ConfigGrpcAccounts::default_stream_maxlen",
        deserialize_with = "deserialize_usize_str"
    )]
    pub stream_maxlen: usize,
    #[serde(default = "ConfigGrpcAccounts::default_stream_data_key")]
    pub stream_data_key: String,

    pub filter: ConfigGrpcRequestAccounts,
}

impl ConfigGrpcAccounts {
    pub const fn default_stream_maxlen() -> usize {
        100_000_000
    }

    pub fn default_stream_data_key() -> String {
        REDIS_STREAM_DATA_KEY.to_owned()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigGrpcTransactions {
    pub stream: String,
    #[serde(
        default = "ConfigGrpcTransactions::default_stream_maxlen",
        deserialize_with = "deserialize_usize_str"
    )]
    pub stream_maxlen: usize,

    pub filter: ConfigGrpcRequestTransactions,
}

impl ConfigGrpcTransactions {
    pub const fn default_stream_maxlen() -> usize {
        10_000_000
    }
}

#[derive(Debug, Clone, Deserialize)]
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
    pub postgres: ConfigIngesterPostgres,
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
pub struct ConfigIngesterPostgres {
    pub url: String,
    #[serde(
        default = "ConfigIngesterPostgres::default_min_connections",
        deserialize_with = "deserialize_usize_str"
    )]
    pub min_connections: usize,
    #[serde(
        default = "ConfigIngesterPostgres::default_max_connections",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_connections: usize,
}

impl ConfigIngesterPostgres {
    pub const fn default_min_connections() -> usize {
        10
    }

    pub const fn default_max_connections() -> usize {
        50
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigIngesterDownloadMetadata {
    pub stream: ConfigIngestStream,
    #[serde(
        default = "ConfigIngesterDownloadMetadata::default_num_threads",
        deserialize_with = "deserialize_usize_str"
    )]
    pub num_threads: usize,
    #[serde(
        default = "ConfigIngesterDownloadMetadata::default_max_attempts",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_attempts: usize,
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
    pub pipeline_max_size: usize,
    #[serde(
        default = "ConfigIngesterDownloadMetadata::default_pipeline_max_idle",
        deserialize_with = "deserialize_duration_str",
        rename = "pipeline_max_idle_ms"
    )]
    pub pipeline_max_idle: Duration,
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
