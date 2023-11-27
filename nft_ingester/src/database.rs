use sqlx::{postgres::{PgPoolOptions, PgConnectOptions}, PgPool, ConnectOptions};

use crate::{
    config::{IngesterConfig, IngesterRole},
};
const BARE_MINIMUM_CONNECTIONS: u32 = 5;
const DEFAULT_MAX: u32 = 125;
pub async fn setup_database(config: IngesterConfig) -> PgPool {
    let max = config.max_postgres_connections.unwrap_or(DEFAULT_MAX);
    if config.role == Some(IngesterRole::All) || config.role == Some(IngesterRole::Ingester) {
        let relative_max: u32 =
            config.get_worker_config().iter().map(|c| c.worker_count).sum();
        let should_be_at_least = relative_max * 5;
        if should_be_at_least > max {
            panic!("Please increase max_postgres_connections to at least {}, at least 5 connections per worker process should be given", should_be_at_least);
        }
    }
    let url = config.get_database_url();
    let mut options: PgConnectOptions = url.parse().unwrap();
    options.log_statements(log::LevelFilter::Trace);

    options.log_slow_statements(log::LevelFilter::Debug, std::time::Duration::from_millis(500));
    
    let pool = PgPoolOptions::new()
        .min_connections(BARE_MINIMUM_CONNECTIONS)
        .max_connections(max)
        .connect_with(options)
        .await
        .unwrap();
    pool
}
