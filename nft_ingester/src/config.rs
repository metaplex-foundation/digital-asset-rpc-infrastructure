use das_metadata_json::sender::SenderArgs;
use figment::{
    providers::{Env, Format, Yaml},
    value::Value,
    Figment,
};
use plerkle_messenger::MessengerConfig;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::Deserialize;
use std::{
    env,
    fmt::{Display, Formatter},
    path::PathBuf,
};
use tracing_subscriber::fmt;

use crate::{error::IngesterError, tasks::BackgroundTaskRunnerConfig};

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct IngesterConfig {
    pub database_config: DatabaseConfig,
    pub messenger_config: MessengerConfig,
    pub env: Option<String>,
    pub rpc_config: RpcConfig,
    pub metrics_port: Option<u16>,
    pub metrics_host: Option<String>,
    pub backfiller: Option<bool>,
    pub backfiller_trees: Option<Vec<String>>,
    pub role: Option<IngesterRole>,
    pub max_postgres_connections: Option<u32>,
    pub worker_config: Option<Vec<WorkerConfig>>,
    pub code_version: Option<&'static str>,
    pub background_task_runner_config: Option<BackgroundTaskRunnerConfig>,
    pub cl_audits: Option<bool>, // save transaction logs for compressed nfts
    pub metadata_json_sender: Option<SenderArgs>,
}

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct WorkerConfig {
    pub stream_name: String,
    pub worker_type: WorkerType,
    pub worker_count: u32,
}

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub enum WorkerType {
    Account,
    Transaction,
}

impl IngesterConfig {
    /// Get the db url out of the dict, this is built a a dict so that future extra db parameters can be easily shoved in.
    /// this panics if the key is not present
    pub fn get_database_url(&self) -> String {
        self.database_config
            .get(DATABASE_URL_KEY)
            .and_then(|u| u.clone().into_string())
            .ok_or(IngesterError::ConfigurationError {
                msg: format!("Database connection string missing: {}", DATABASE_URL_KEY),
            })
            .unwrap()
    }

    pub fn get_rpc_url(&self) -> String {
        self.rpc_config
            .get(RPC_URL_KEY)
            .and_then(|u| u.clone().into_string())
            .ok_or(IngesterError::ConfigurationError {
                msg: format!("RPC connection string missing: {}", RPC_URL_KEY),
            })
            .unwrap()
    }

    pub fn get_messenger_client_config(&self) -> MessengerConfig {
        let mut mc = self.messenger_config.clone();
        mc.connection_config
            .insert("consumer_id".to_string(), Value::from(rand_string()));
        mc
    }

    pub fn get_worker_config(&self) -> Vec<WorkerConfig> {
        if let Some(wc) = &self.worker_config {
            wc.to_vec()
        } else {
            vec![
                WorkerConfig {
                    stream_name: "ACC".to_string(),
                    worker_count: 2,
                    worker_type: WorkerType::Account,
                },
                WorkerConfig {
                    stream_name: "TXN".to_string(),
                    worker_count: 2,
                    worker_type: WorkerType::Transaction,
                },
            ]
        }
    }

    pub fn get_worker_count(&self) -> u32 {
        let mut count = 0;
        for wc in self.get_worker_config() {
            count += wc.worker_count;
        }
        count
    }
}

// Types and constants used for Figment configuration items.
pub type DatabaseConfig = figment::value::Dict;

pub const DATABASE_URL_KEY: &str = "url";
pub const DATABASE_LISTENER_CHANNEL_KEY: &str = "listener_channel";

pub type RpcConfig = figment::value::Dict;

pub const RPC_URL_KEY: &str = "url";
pub const RPC_COMMITMENT_KEY: &str = "commitment";
pub const CODE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize, Default, PartialEq, Eq, Debug, Clone)]
pub enum IngesterRole {
    #[default]
    All,
    Backfiller,
    BackgroundTaskRunner,
    Ingester,
}

impl Display for IngesterRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IngesterRole::All => write!(f, "All"),
            IngesterRole::Backfiller => write!(f, "Backfiller"),
            IngesterRole::BackgroundTaskRunner => write!(f, "BackgroundTaskRunner"),
            IngesterRole::Ingester => write!(f, "Ingester"),
        }
    }
}

pub fn rand_string() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(30)
        .map(char::from)
        .collect()
}

pub fn setup_config(config_file: Option<&PathBuf>) -> IngesterConfig {
    let mut figment = Figment::new().join(Env::prefixed("INGESTER_"));

    if let Some(config_file) = config_file {
        figment = figment.join(Yaml::file(config_file));
    }

    let mut config: IngesterConfig = figment
        .extract()
        .map_err(|config_error| IngesterError::ConfigurationError {
            msg: format!("{}", config_error),
        })
        .unwrap();
    config.code_version = Some(CODE_VERSION);
    config
}

pub fn init_logger() {
    let env_filter = env::var("RUST_LOG").unwrap_or("info".to_string());
    let t = tracing_subscriber::fmt().with_env_filter(env_filter);
    t.event_format(fmt::format::json()).init();
}
