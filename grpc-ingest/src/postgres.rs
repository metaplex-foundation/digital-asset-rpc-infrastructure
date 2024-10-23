use {
    crate::{
        config::ConfigIngesterPostgres,
        prom::{pgpool_connections_set, PgpoolConnectionsKind},
    },
    sqlx::{
        postgres::{PgConnectOptions, PgPoolOptions},
        PgPool,
    },
};

pub async fn create_pool(config: ConfigIngesterPostgres) -> anyhow::Result<PgPool> {
    let options: PgConnectOptions = config.url.parse()?;
    PgPoolOptions::new()
        .min_connections(config.min_connections.try_into()?)
        .max_connections(config.max_connections.try_into()?)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime)
        .connect_with(options)
        .await
        .map_err(Into::into)
}

pub fn report_pgpool(pgpool: PgPool) {
    pgpool_connections_set(PgpoolConnectionsKind::Total, pgpool.size() as usize);
    pgpool_connections_set(PgpoolConnectionsKind::Idle, pgpool.num_idle());
}
