use {
    crate::{
        config::ConfigIngesterPostgres,
        prom::{pgpool_connections_set, PgpoolConnectionsKind},
    },
    sqlx::{
        postgres::{PgConnectOptions, PgPoolOptions},
        PgPool,
    },
    tokio::time::{sleep, Duration},
};

pub async fn create_pool(config: ConfigIngesterPostgres) -> anyhow::Result<PgPool> {
    let options: PgConnectOptions = config.url.parse()?;
    PgPoolOptions::new()
        .min_connections(config.min_connections.try_into()?)
        .max_connections(config.max_connections.try_into()?)
        .connect_with(options)
        .await
        .map_err(Into::into)
}

pub async fn metrics_pgpool(pgpool: PgPool) {
    loop {
        pgpool_connections_set(PgpoolConnectionsKind::Total, pgpool.size() as usize);
        pgpool_connections_set(PgpoolConnectionsKind::Idle, pgpool.num_idle());
        sleep(Duration::from_millis(100)).await;
    }
}
