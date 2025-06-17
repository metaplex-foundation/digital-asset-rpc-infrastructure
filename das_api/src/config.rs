use std::{net::SocketAddr, str::FromStr};

use crate::error::DasApiError;
use {
    figment::{providers::Env, Figment},
    serde::Deserialize,
};

#[derive(Deserialize, Default)]
pub struct Config {
    pub database_url: String,
    pub max_database_connections: Option<u32>,
    pub max_request_connections: Option<u32>,
    pub otlp_collector_host: Option<String>,
    pub otlp_collector_port: Option<u16>,
    pub metrics_host: Option<String>,
    pub metrics_port: Option<u16>,
    pub server_port: Option<u16>,
    pub env: Option<String>,
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
