use {
    anyhow::Context,
    serde::{de, Deserialize},
    std::{collections::HashMap, net::SocketAddr, path::Path, time::Duration},
    tokio::fs,
    yellowstone_grpc_proto::prelude::SubscribeRequest,
    yellowstone_grpc_tools::config::{
        deserialize_usize_str, ConfigGrpcRequestAccounts, ConfigGrpcRequestCommitment,
        ConfigGrpcRequestTransactions, GrpcRequestToProto,
    },
};

pub const REDIS_STREAM_ACCOUNTS: &str = "ACCOUNTS";
pub const REDIS_STREAM_TRANSACTIONS: &str = "TRANSACTIONS";
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

impl ConfigGrpc {
    pub fn create_subscribe_request(&self) -> SubscribeRequest {
        let mut accounts = HashMap::new();
        for (i, item) in self.accounts.filters.iter().enumerate() {
            accounts.insert(i.to_string(), item.clone().to_proto());
        }

        let mut transactions = HashMap::new();
        for (i, item) in self.transactions.filters.iter().enumerate() {
            transactions.insert(i.to_string(), item.clone().to_proto());
        }

        SubscribeRequest {
            slots: HashMap::new(),
            accounts,
            transactions,
            entry: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta: HashMap::new(),
            commitment: Some(self.commitment.to_proto() as i32),
            accounts_data_slice: vec![],
            ping: None,
        }
    }
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
        REDIS_STREAM_DATA_KEY.to_owned()
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
        REDIS_STREAM_DATA_KEY.to_owned()
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

#[derive(Debug, Deserialize)]
pub struct ConfigIngester {
    pub redis: ConfigIngesterRedis,
    pub postgres: ConfigIngesterPostgres,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Clone)]
pub struct ConfigIngesterRedisStream {
    pub stream_type: ConfigIngesterRedisStreamType,
    pub stream: String,
    pub data_key: String,
    pub xack_batch_max_size: usize,
    pub xack_batch_max_idle: Duration,
    pub xack_max_in_process: usize,
}

impl<'de> Deserialize<'de> for ConfigIngesterRedisStream {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        struct Raw {
            #[serde(rename = "type")]
            pub stream_type: ConfigIngesterRedisStreamType,
            pub stream: Option<String>,
            pub data_key: Option<String>,
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
            stream: raw.stream.unwrap_or_else(|| match raw.stream_type {
                ConfigIngesterRedisStreamType::Account => REDIS_STREAM_ACCOUNTS.to_owned(),
                ConfigIngesterRedisStreamType::Transaction => REDIS_STREAM_TRANSACTIONS.to_owned(),
            }),
            data_key: raw
                .data_key
                .unwrap_or_else(|| REDIS_STREAM_DATA_KEY.to_owned()),
            xack_batch_max_size: raw.xack_batch_max_size,
            xack_batch_max_idle: raw.xack_batch_max_idle,
            xack_max_in_process: raw.xack_max_in_process,
        })
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigIngesterRedisStreamType {
    Account,
    Transaction,
}

#[derive(Debug, Deserialize)]
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
        25
    }
}
