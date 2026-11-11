use crate::error::DasApiError;
use std::num::NonZeroU32;
use {
    figment::{providers::Env, Figment},
    serde::Deserialize,
};

#[derive(Deserialize, Default)]
pub struct Config {
    pub database_url: String,
    pub max_postgres_connections: Option<NonZeroU32>,
    pub metrics_port: Option<u16>,
    pub metrics_host: Option<String>,
    pub server_port: u16,
    pub env: Option<String>,
}

impl Config {
    pub fn database_max_connections(&self) -> u32 {
        self.max_postgres_connections.map_or(250, NonZeroU32::get)
    }
}

pub fn load_config() -> Result<Config, DasApiError> {
    Figment::new()
        .join(Env::prefixed("APP_"))
        .extract()
        .map_err(|config_error| DasApiError::ConfigurationError(config_error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment::providers::Serialized;

    #[test]
    fn database_pool_budget_is_configurable_and_must_be_nonzero() {
        let config: Config = Figment::from(Serialized::defaults(serde_json::json!({
            "database_url": "postgres://localhost/solana",
            "server_port": 9090,
            "max_postgres_connections": 25,
        })))
        .extract()
        .unwrap();
        assert_eq!(config.database_max_connections(), 25);

        let invalid = Figment::from(Serialized::defaults(serde_json::json!({
            "database_url": "postgres://localhost/solana",
            "server_port": 9090,
            "max_postgres_connections": 0,
        })))
        .extract::<Config>();
        assert!(invalid.is_err());
        assert_eq!(Config::default().database_max_connections(), 250);
    }
}
