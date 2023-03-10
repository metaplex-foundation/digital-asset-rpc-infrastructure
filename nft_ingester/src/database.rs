use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::{
    config::{IngesterConfig, IngesterRole},
    error,
};
const BARE_MINIMUM_CONNECTIONS: u32 = 5;
const DEFAULT_MAX: u32 = 125;
pub async fn setup_database(config: IngesterConfig) -> PgPool {
    let max = config.max_postgres_connections.unwrap_or(125);
    if config.role == Some(IngesterRole::All) || config.role == Some(IngesterRole::Ingester) {
        let relative_max =
            config.get_account_stream_worker_count() + config.get_transaction_stream_worker_count();
        let should_be_at_least = relative_max * 5;
        if relative_max * 5 < max {
            panic!("Please increase max_postgres_connections to at least {}, at least 5 connections per worker process should be given", should_be_at_least);
        }
    }
    let url = config.get_database_url();
    PgPoolOptions::new()
        .min_connections(BARE_MINIMUM_CONNECTIONS)
        .max_connections(max)
        .connect(&url)
        .await
        .unwrap()
}
