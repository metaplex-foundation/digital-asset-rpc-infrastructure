use anyhow::Result;
use clap::Parser;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};

#[derive(Debug, Parser, Clone)]
pub struct PoolArgs {
    /// The database URL.
    #[arg(long, env)]
    pub database_url: String,
    /// The maximum number of connections to the database.
    #[arg(long, env, default_value = "125")]
    pub database_max_connections: u32,
    /// The minimum number of connections to the database.
    #[arg(long, env, default_value = "5")]
    pub database_min_connections: u32,
}

///// Establishes a connection to the database using the provided configuration.
/////
///// # Arguments
/////
///// * `config` - A `PoolArgs` struct containing the database URL and the minimum and maximum number of connections.
/////
///// # Returns
/////
///// * `Result<DatabaseConnection, DbErr>` - On success, returns a `DatabaseConnection`. On failure, returns a `DbErr`.
pub async fn connect_db(config: &PoolArgs) -> Result<PgPool, sqlx::Error> {
    let options: PgConnectOptions = config.database_url.parse()?;

    PgPoolOptions::new()
        .min_connections(config.database_min_connections)
        .max_connections(config.database_max_connections)
        .connect_with(options)
        .await
}
