use std::{net::SocketAddr, str::FromStr};

use das_core::PoolArgs;

use crate::error::DasApiError;
use {
    figment::{providers::Env, Figment},
    serde::Deserialize,
};

#[derive(Deserialize, Default)]
pub struct Config {
    pub database_url: String,
    pub database_max_connections: Option<u32>,
    pub database_min_connections: Option<u32>,
    pub database_max_lifetime: Option<u64>,
    pub database_acquire_timeout: Option<u64>,
    pub database_idle_timeout: Option<u64>,
    pub max_request_connections: Option<u32>,
    pub otlp_collector_host: Option<String>,
    pub otlp_collector_port: Option<u16>,
    pub metrics_host: Option<String>,
    pub metrics_port: Option<u16>,
    pub server_port: Option<u16>,
    pub env: Option<String>,
}

impl From<Config> for PoolArgs {
    fn from(value: Config) -> Self {
        Self {
            database_acquire_timeout: value
                .database_acquire_timeout
                .unwrap_or_else(PoolArgs::default_database_acquire_timeout),
            database_url: value.database_url,
            database_max_connections: value
                .database_max_connections
                .unwrap_or_else(PoolArgs::default_database_max_connections),
            database_min_connections: value
                .database_min_connections
                .unwrap_or_else(PoolArgs::default_database_min_connections),
            database_idle_timeout: value
                .database_idle_timeout
                .unwrap_or_else(PoolArgs::default_database_idle_timeout),
            database_max_lifetime: value
                .database_max_lifetime
                .unwrap_or_else(PoolArgs::default_database_max_lifetime),
        }
    }
}

impl Config {
    pub fn get_otlp_collector_endpoint(&self) -> String {
        format!(
            "{}:{}",
            self.otlp_collector_host
                .as_ref()
                .map_or("http://0.0.0.0", |v| v),
            self.otlp_collector_port
                .unwrap_or(DEFAULT_OTLP_COLLECTOR_PORT)
        )
    }

    pub fn get_prom_metrics_collector_endpoint(&self) -> SocketAddr {
        SocketAddr::from_str(&format!(
            "{}:{}",
            self.metrics_host.as_ref().map_or("0.0.0.0", |v| v),
            self.metrics_port
                .unwrap_or(DEFAULT_PROM_METRICS_COLLECTOR_PORT)
        ))
        .expect("error getting endpoint")
    }
}

pub fn load_config() -> Result<Config, DasApiError> {
    Figment::new()
        .join(Env::prefixed("APP_"))
        .extract()
        .map_err(|config_error| DasApiError::ConfigurationError(config_error.to_string()))
}

pub const DEFAULT_OTLP_COLLECTOR_PORT: u16 = 4318;
pub const DEFAULT_PROM_METRICS_COLLECTOR_PORT: u16 = 8875;
pub const DEFAULT_SERVER_PORT: u16 = 3000;
