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

pub const REDIS_STREAM_ACCOUNTS: &str = "ACCOUNTS";
pub const REDIS_STREAM_TRANSACTIONS: &str = "TRANSACTIONS";

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

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ConfigPrometheus {
    pub prometheus: Option<SocketAddr>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigGrpc {
    pub endpoint: String,
    pub x_token: Option<String>,

    pub commitment: ConfigGrpcRequestCommitment,
    pub accounts: ConfigGrpcAccounts,
    pub transactions: ConfigGrpcTransactions,

    pub redis: ConfigGrpcRedis,
}

#[derive(Debug, Deserialize)]
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

    pub filters: Vec<ConfigGrpcRequestAccounts>,
}

impl ConfigGrpcAccounts {
    pub fn default_stream() -> String {
        REDIS_STREAM_ACCOUNTS.to_owned()
    }

    pub const fn default_stream_maxlen() -> usize {
        100_000_000
    }

    pub fn default_stream_data_key() -> String {
        "data".to_owned()
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigGrpcTransactions {
    #[serde(default = "ConfigGrpcTransactions::default_stream")]
    pub stream: String,
    #[serde(
        default = "ConfigGrpcTransactions::default_stream_maxlen",
        deserialize_with = "deserialize_usize_str"
    )]
    pub stream_maxlen: usize,
    #[serde(default = "ConfigGrpcTransactions::default_stream_data_key")]
    pub stream_data_key: String,

    pub filters: Vec<ConfigGrpcRequestTransactions>,
}

impl ConfigGrpcTransactions {
    pub fn default_stream() -> String {
        REDIS_STREAM_TRANSACTIONS.to_owned()
    }

    pub const fn default_stream_maxlen() -> usize {
        10_000_000
    }

    pub fn default_stream_data_key() -> String {
        "data".to_owned()
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigGrpcRedis {
    pub url: String,
    #[serde(
        default = "ConfigGrpcRedis::default_pipeline_max_size",
        deserialize_with = "deserialize_usize_str"
    )]
    pub pipeline_max_size: usize,
    #[serde(
        default = "ConfigGrpcRedis::default_pipeline_max_idle_ms",
        deserialize_with = "deserialize_duration_str"
    )]
    pub pipeline_max_idle_ms: Duration,
    #[serde(
        default = "ConfigGrpcRedis::max_xadd_in_process",
        deserialize_with = "deserialize_usize_str"
    )]
    pub max_xadd_in_process: usize,
}

impl ConfigGrpcRedis {
    pub const fn default_pipeline_max_size() -> usize {
        10
    }

    pub const fn default_pipeline_max_idle_ms() -> Duration {
        Duration::from_millis(10)
    }

    pub const fn max_xadd_in_process() -> usize {
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

#[derive(Debug, Deserialize)]
pub struct ConfigIngester {
    //
}
