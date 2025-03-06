use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use sea_orm::{DatabaseConnection, MockDatabase, MockDatabaseConnection, SqlxPostgresConnector};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool, Pool, Postgres,
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
pub async fn connect_db(config: PoolArgs) -> Result<PgPool, sqlx::Error> {
    let options: PgConnectOptions = config.database_url.parse()?;

    PgPoolOptions::new()
        .min_connections(config.database_min_connections)
        .max_connections(config.database_max_connections)
        .connect_with(options)
        .await
}

pub trait DbConn: Clone + Send + 'static {
    fn connection(&self) -> DatabaseConnection;
}

#[derive(Clone)]
pub struct DbPool<P: DbConn> {
    pub pool: P,
}

impl DbPool<PostgresPool> {
    pub const fn from(pool: Pool<Postgres>) -> DbPool<PostgresPool> {
        DbPool {
            pool: PostgresPool::new(pool),
        }
    }
}

impl DbPool<MockDb> {
    pub fn from(mock_db: MockDatabase) -> DbPool<MockDb> {
        DbPool {
            pool: MockDb(Arc::new(MockDatabaseConnection::new(mock_db))),
        }
    }
}

impl<P: DbConn> DbPool<P> {
    pub fn connection(&self) -> DatabaseConnection {
        self.pool.connection()
    }
}

#[derive(Clone)]
pub struct PostgresPool(sqlx::PgPool);

impl PostgresPool {
    pub const fn new(pool: sqlx::PgPool) -> Self {
        Self(pool)
    }
}

#[derive(Clone)]
pub struct MockDb(Arc<MockDatabaseConnection>);

impl DbConn for MockDb {
    fn connection(&self) -> DatabaseConnection {
        DatabaseConnection::MockDatabaseConnection(Arc::clone(&self.0))
    }
}

impl DbConn for PostgresPool {
    fn connection(&self) -> DatabaseConnection {
        SqlxPostgresConnector::from_sqlx_postgres_pool(self.0.clone())
    }
}
