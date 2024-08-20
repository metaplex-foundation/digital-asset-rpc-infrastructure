use {
    anyhow::Context,
    serde::{de, Deserialize},
    std::{net::SocketAddr, path::Path, time::Duration},
    tokio::fs,
    tracing::warn,
    yellowstone_grpc_tools::config::{
        deserialize_usize_str, ConfigGrpcRequestAccounts, ConfigGrpcRequestCommitment,
        ConfigGrpcRequestTransactions,
    },
};

pub const REDIS_STREAM_ACCOUNTS: &str = "ACCOUNTS";
pub const REDIS_STREAM_TRANSACTIONS: &str = "TRANSACTIONS";
pub const REDIS_STREAM_METADATA_JSON: &str = "METADATA_JSON";
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
    #[serde(default = "ConfigIngestStream::default_xack_batch_max_size")]
    pub xack_batch_max_size: usize,
    #[serde(
        default = "ConfigIngestStream::default_xack_batch_max_idle",
        deserialize_with = "deserialize_duration_str",
        rename = "xack_batch_max_idle_ms"
    )]
    pub xack_batch_max_idle: Duration,
    #[serde(default = "ConfigIngestStream::default_batch_size")]
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
        Duration::from_millis(100)
    }

    pub fn default_group() -> String {
        "ingester".to_owned()
    }

    pub fn default_consumer() -> String {
        "consumer".to_owned()
    }

    pub const fn default_xack_batch_max_size() -> usize {
        100
    }

    pub const fn default_batch_size() -> usize {
        100
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

    #[serde(
        default = "ConfigGrpc::default_geyser_update_message_buffer_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub geyser_update_message_buffer_size: usize,

    #[serde(
        default = "ConfigGrpc::solana_seen_event_cache_max_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub solana_seen_event_cache_max_size: usize,

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

    pub const fn default_geyser_update_message_buffer_size() -> usize {
        100_000
    }

    pub const fn solana_seen_event_cache_max_size() -> usize {
        10_000
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigGrpcAccounts {
    #[serde(default = "ConfigGrpcAccounts::default_stream")]
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
    pub fn default_stream() -> String {
        REDIS_STREAM_ACCOUNTS.to_owned()
    }

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
        default = "ConfigGrpcRedis::default_pipeline_max_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub pipeline_max_size: usize,
    #[serde(
        default = "ConfigGrpcRedis::default_pipeline_max_idle",
        deserialize_with = "deserialize_duration_str",
        rename = "pipeline_max_idle_ms"
    )]
    pub pipeline_max_idle: Duration,
    #[serde(
        default = "ConfigGrpcRedis::default_max_xadd_in_process",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_xadd_in_process: usize,
}

impl ConfigGrpcRedis {
    pub const fn default_pipeline_max_size() -> usize {
        10
    }

    pub const fn default_pipeline_max_idle() -> Duration {
        Duration::from_millis(10)
    }

    pub const fn default_max_xadd_in_process() -> usize {
        100
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
    pub redis: ConfigIngesterRedis,
    pub postgres: ConfigIngesterPostgres,
    pub download_metadata: ConfigIngesterDownloadMetadata,
    pub program_transformer: ConfigIngesterProgramTransformer,
    pub accounts: ConfigIngestStream,
    pub transactions: ConfigIngestStream,
}

impl ConfigIngester {
    pub fn check(&self) {
        let total_threads = self.program_transformer.account_num_threads
            + self.program_transformer.transaction_num_threads
            + self.program_transformer.metadata_json_num_threads;

        if self.postgres.max_connections < total_threads {
            warn!(
                "postgres.max_connections ({}) should be more than the number of threads ({})",
                self.postgres.max_connections, total_threads
            );
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigIngesterRedis {
    pub url: String,
    #[serde(default = "ConfigIngesterRedis::default_group")]
    pub group: String,
    #[serde(default = "ConfigIngesterRedis::default_consumer")]
    pub consumer: String,
    pub streams: Vec<ConfigIngesterRedisStream>,
    #[serde(
        default = "ConfigIngesterRedis::default_prefetch_queue_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub prefetch_queue_size: usize,
    #[serde(
        default = "ConfigIngesterRedis::default_xpending_max",
        deserialize_with = "deserialize_usize_str"
    )]
    pub xpending_max: usize,
    #[serde(default = "ConfigIngesterRedis::default_xpending_only")]
    pub xpending_only: bool,
    #[serde(
        default = "ConfigIngesterRedis::default_xreadgroup_max",
        deserialize_with = "deserialize_usize_str"
    )]
    pub xreadgroup_max: usize,
}

impl ConfigIngesterRedis {
    pub fn default_group() -> String {
        "ingester".to_owned()
    }

    pub fn default_consumer() -> String {
        "consumer".to_owned()
    }

    pub const fn default_prefetch_queue_size() -> usize {
        1_000
    }

    pub const fn default_xpending_max() -> usize {
        100
    }

    pub const fn default_xpending_only() -> bool {
        false
    }

    pub const fn default_xreadgroup_max() -> usize {
        1_000
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConfigIngesterRedisStream {
    pub stream_type: ConfigIngesterRedisStreamType,
    pub stream: &'static str,
    pub xack_batch_max_size: usize,
    pub xack_batch_max_idle: Duration,
    pub xack_max_in_process: usize,
}

impl<'de> Deserialize<'de> for ConfigIngesterRedisStream {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        #[derive(Debug, Clone, Copy, Deserialize)]
        struct Raw {
            #[serde(rename = "type")]
            pub stream_type: ConfigIngesterRedisStreamType,
            #[serde(
                default = "default_xack_batch_max_size",
                deserialize_with = "deserialize_usize_str"
            )]
            pub xack_batch_max_size: usize,
            #[serde(
                default = "default_xack_batch_max_idle",
                deserialize_with = "deserialize_duration_str",
                rename = "xack_batch_max_idle_ms"
            )]
            pub xack_batch_max_idle: Duration,
            #[serde(
                default = "default_xack_max_in_process",
                deserialize_with = "deserialize_usize_str"
            )]
            pub xack_max_in_process: usize,
        }

        const fn default_xack_batch_max_size() -> usize {
            100
        }

        const fn default_xack_batch_max_idle() -> Duration {
            Duration::from_millis(10)
        }

        const fn default_xack_max_in_process() -> usize {
            100
        }

        let raw = Raw::deserialize(deserializer)?;

        Ok(Self {
            stream_type: raw.stream_type,
            stream: match raw.stream_type {
                ConfigIngesterRedisStreamType::Account => REDIS_STREAM_ACCOUNTS,
                ConfigIngesterRedisStreamType::Transaction => REDIS_STREAM_TRANSACTIONS,
                ConfigIngesterRedisStreamType::MetadataJson => REDIS_STREAM_METADATA_JSON,
            },
            xack_batch_max_size: raw.xack_batch_max_size,
            xack_batch_max_idle: raw.xack_batch_max_idle,
            xack_max_in_process: raw.xack_max_in_process,
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigIngesterRedisStreamType {
    Account,
    Transaction,
    MetadataJson,
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

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigIngesterProgramTransformer {
    #[serde(
        default = "ConfigIngesterProgramTransformer::default_max_tasks_in_process",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_tasks_in_process: usize,
    #[serde(
        default = "ConfigIngesterProgramTransformer::default_account_num_threads",
        deserialize_with = "deserialize_usize_str"
    )]
    pub account_num_threads: usize,
    #[serde(
        default = "ConfigIngesterProgramTransformer::default_transaction_num_threads",
        deserialize_with = "deserialize_usize_str"
    )]
    pub transaction_num_threads: usize,
    #[serde(
        default = "ConfigIngesterProgramTransformer::default_metadata_json_num_threads",
        deserialize_with = "deserialize_usize_str"
    )]
    pub metadata_json_num_threads: usize,
}

impl ConfigIngesterProgramTransformer {
    pub const fn default_account_num_threads() -> usize {
        5
    }

    pub const fn default_transaction_num_threads() -> usize {
        1
    }

    pub const fn default_metadata_json_num_threads() -> usize {
        1
    }

    pub const fn default_max_tasks_in_process() -> usize {
        40
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigIngesterDownloadMetadata {
    pub stream_config: ConfigIngestStream,
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
    #[serde(default = "ConfigIngesterDownloadMetadata::default_stream")]
    pub stream: String,
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

    pub fn default_stream() -> String {
        REDIS_STREAM_METADATA_JSON.to_owned()
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
pub struct ConfigDownloadMetadata {
    pub postgres: ConfigIngesterPostgres,
    pub download_metadata: ConfigDownloadMetadataOpts,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ConfigDownloadMetadataOpts {
    #[serde(
        default = "ConfigDownloadMetadataOpts::default_max_in_process",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_in_process: usize,
    #[serde(
        default = "ConfigDownloadMetadataOpts::default_prefetch_queue_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub prefetch_queue_size: usize,
    #[serde(
        default = "ConfigDownloadMetadataOpts::default_limit_to_fetch",
        deserialize_with = "deserialize_usize_str"
    )]
    pub limit_to_fetch: usize,
    #[serde(
        default = "ConfigDownloadMetadataOpts::default_wait_tasks_max_idle",
        deserialize_with = "deserialize_duration_str",
        rename = "wait_tasks_max_idle_ms"
    )]
    pub wait_tasks_max_idle: Duration,
    #[serde(
        default = "ConfigDownloadMetadataOpts::default_download_timeout",
        deserialize_with = "deserialize_duration_str",
        rename = "download_timeout_ms"
    )]
    pub download_timeout: Duration,

    pub stream: ConfigIngesterRedisStream,
}

impl ConfigDownloadMetadataOpts {
    pub const fn default_max_in_process() -> usize {
        50
    }

    pub const fn default_prefetch_queue_size() -> usize {
        100
    }

    pub const fn default_limit_to_fetch() -> usize {
        200
    }

    pub const fn default_wait_tasks_max_idle() -> Duration {
        Duration::from_millis(100)
    }

    pub const fn default_download_timeout() -> Duration {
        Duration::from_millis(5_000)
    }
}
