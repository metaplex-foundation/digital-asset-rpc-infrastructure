use crate::{
    config::ConfigSnapshot,
    grpc,
    redis::{IngestStream, SnapshotHandle},
};
use das_core::{DownloadMetadataJsonRetryConfig, MetadataJsonDownloadWorker};
use digital_asset_types::dao::{account_snapshots, token_accounts, tokens};
use futures::stream::StreamExt;
use sea_orm::{
    sea_query::{Expr, PostgresQueryBuilder, Query},
    EntityTrait, JoinType, SqlxPostgresConnector,
};
use sea_orm::{ConnectionTrait, Statement};
use tracing::{info, warn};
use {
    crate::{postgres::create_pool as pg_create_pool, util::create_shutdown},
    das_core::create_download_metadata_notifier,
    program_transformers::ProgramTransformer,
    redis::AsyncCommands,
    std::sync::Arc,
    tokio::time::{sleep, Duration},
};

pub async fn run(config: ConfigSnapshot) -> anyhow::Result<()> {
    let redis_client = redis::Client::open(config.redis.url.clone())?;
    let connection = redis_client.get_multiplexed_tokio_connection().await?;
    let pool = pg_create_pool(config.postgres.clone()).await?;

    let (download_metadata_sender, download_metadata_worker) = MetadataJsonDownloadWorker::build()
        .pool(pool.clone())
        .request_timeout(
            config
                .download_metadata
                .metadata_json_download_worker_request_timeout,
        )
        .worker_count(config.download_metadata.metadata_json_download_worker_count)
        .retry(Arc::new(DownloadMetadataJsonRetryConfig::default()))
        .build()?
        .run();

    let download_metadata_notifier =
        create_download_metadata_notifier(download_metadata_sender).await;

    let program_transformer = Arc::new(ProgramTransformer::new(
        pool.clone(),
        download_metadata_notifier,
    ));

    let account_snapshot_stream = IngestStream::build()
        .config(config.snapshots.clone())
        .connection(connection.clone())
        .handler(SnapshotHandle::new(Arc::clone(&program_transformer)))
        .start()
        .await?;

    grpc::run(config.clone().into()).await?;

    let mut shutdown = create_shutdown()?;
    let mut connection = connection.clone();
    loop {
        tokio::select! {
            _ = shutdown.next() => {
                warn!(
                    action = "shutdown_signal_received",
                    message = "Shutdown signal received, stopping ingest streams",
                );
                break;
            }

            Ok(len) = connection.xlen::<String, usize>(config.snapshots.name.clone()) => {
                if len == 0 {
                    break;
                }
            }

            _ = sleep(Duration::from_millis(100)) => {}
        }
    }
    account_snapshot_stream.stop().await?;
    download_metadata_worker.stop().await?;

    let db_connection = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

    let (sql, values) = Query::delete()
        .from_table(token_accounts::Entity)
        .and_where(
            Expr::col(token_accounts::Column::Pubkey).in_subquery(
                Query::select()
                    .columns([(token_accounts::Entity, token_accounts::Column::Pubkey)])
                    .from(token_accounts::Entity)
                    .join(
                        JoinType::LeftJoin,
                        account_snapshots::Entity,
                        Expr::tbl(account_snapshots::Entity, account_snapshots::Column::Pubkey).eq(
                            Expr::tbl(token_accounts::Entity, token_accounts::Column::Pubkey),
                        ),
                    )
                    .and_where(
                        Expr::tbl(account_snapshots::Entity, account_snapshots::Column::Pubkey)
                            .is_null(),
                    )
                    .to_owned(),
            ),
        )
        .build(PostgresQueryBuilder);

    let token_accounts_deleted = db_connection
        .execute(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            &sql,
            values,
        ))
        .await?
        .rows_affected();

    info!(
        "action=delete_token_accounts count={}",
        token_accounts_deleted
    );

    let (sql, values) = Query::delete()
        .from_table(tokens::Entity)
        .and_where(
            Expr::col(tokens::Column::Mint).in_subquery(
                Query::select()
                    .columns([(tokens::Entity, tokens::Column::Mint)])
                    .from(tokens::Entity)
                    .join(
                        JoinType::LeftJoin,
                        account_snapshots::Entity,
                        Expr::tbl(account_snapshots::Entity, account_snapshots::Column::Pubkey)
                            .eq(Expr::tbl(tokens::Entity, tokens::Column::Mint)),
                    )
                    .and_where(
                        Expr::tbl(account_snapshots::Entity, account_snapshots::Column::Pubkey)
                            .is_null(),
                    )
                    .to_owned(),
            ),
        )
        .build(PostgresQueryBuilder);

    let tokens_deleted = db_connection
        .execute(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            &sql,
            values,
        ))
        .await?
        .rows_affected();

    info!("action=delete_tokens count={}", tokens_deleted);

    account_snapshots::Entity::delete_many()
        .exec(&db_connection)
        .await?;

    let mut con = connection.clone();
    let _: () = con.del(config.snapshots.name.clone()).await?;

    Ok(())
}
