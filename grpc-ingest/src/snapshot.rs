use crate::{
    config::ConfigSnapshot,
    grpc,
    redis::{IngestStream, SnapshotHandle},
};
use anyhow::anyhow;
use das_core::{DownloadMetadataJsonRetryConfig, MetadataJsonDownloadWorker};
use digital_asset_types::dao::{account_snapshots, token_accounts, tokens};
use futures::stream::StreamExt;
use program_transformers::AccountInfo;
use sea_orm::{
    sea_query::{Expr, PostgresQueryBuilder, Query},
    EntityTrait, JoinType, SqlxPostgresConnector,
};
use sea_orm::{ConnectionTrait, Statement};
use sqlx::PgPool;
use tokio::{
    sync::{mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};
use tracing::{error, info, warn};
use {
    crate::{postgres::create_pool as pg_create_pool, util::create_shutdown},
    das_core::create_download_metadata_notifier,
    program_transformers::ProgramTransformer,
    redis::AsyncCommands,
    std::sync::Arc,
    tokio::time::{sleep, Duration},
};

const DEFAULT_PROGRAM_TRANSFORMER_MAX_WORKERS: usize = 20;
const DEFAULT_PROGRAM_TRANSFORMER_BUFFER_CAPACITY: usize = 10_000;

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
    let program_transformer_runner = ProgramTransformerRunner::builder()
        .max_workers(config.program_transform.max_workers)
        .buffer_capacity(config.program_transform.buffer_capacity)
        .program_transformer(program_transformer)
        .build()?;
    let program_transformer_runner_sender = program_transformer_runner.sender();

    let mut account_snapshot_writer = AccountSnapshotWriter::builder()
        .pool(pool.clone())
        .max_workers(config.snapshot_write.max_workers)
        .batch_size(config.snapshot_write.batch_size)
        .channel_capacity(config.snapshot_write.channel_capacity)
        .program_transformer_runner_sender(program_transformer_runner_sender)
        .build();
    let account_snapshot_writer_sender = account_snapshot_writer.sender();
    let mut account_snapshot_writer_error_receiver = account_snapshot_writer
        .take_error_receiver()
        .expect("Error receiver already taken");

    let account_snapshot_stream = IngestStream::build()
        .config(config.snapshot_process.clone())
        .connection(connection.clone())
        .handler(SnapshotHandle::new(account_snapshot_writer_sender))
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

                return Ok(());
            }

            Ok(len) = connection.xlen::<String, usize>(config.snapshot_process.name.clone()) => {
                if len == 0 {
                    break;
                }
            }

            _ = account_snapshot_writer_error_receiver.recv() => {
                return Err(anyhow!("Failed to write a snapshot batch"))
            }

            _ = sleep(Duration::from_millis(100)) => {}
        }
    }

    account_snapshot_stream.stop().await?;

    account_snapshot_writer.shutdown().await;

    program_transformer_runner.shutdown().await;

    download_metadata_worker.stop().await?;

    let db_connection = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

    let slot = account_snapshots::Entity::find()
        .one(&db_connection)
        .await?
        .map(|model| model.slot)
        .ok_or(anyhow!("No snapshot slot"))?;

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
                    .and_where(
                        Expr::tbl(token_accounts::Entity, token_accounts::Column::SlotUpdated)
                            .lte(slot),
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
                    .and_where(Expr::tbl(tokens::Entity, tokens::Column::SlotUpdated).lte(slot))
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
    let _: () = con.del(config.snapshot_process.name.clone()).await?;

    Ok(())
}

pub struct AccountSnapshotWriter {
    update_sender: mpsc::Sender<AccountInfo>,
    stop_sender: Option<oneshot::Sender<()>>,
    error_receiver: Option<mpsc::Receiver<()>>,
    handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Clone, Default)]
pub struct AccountSnapshotWriterBuilder {
    channel_capacity: Option<usize>,
    batch_size: Option<usize>,
    pool: Option<PgPool>,
    max_workers: Option<usize>,
    program_transformer_runner_sender: Option<mpsc::Sender<Vec<AccountInfo>>>,
}

impl AccountSnapshotWriterBuilder {
    pub const fn batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    pub const fn channel_capacity(mut self, channel_capacity: usize) -> Self {
        self.channel_capacity = Some(channel_capacity);
        self
    }

    pub fn pool(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub const fn max_workers(mut self, max_workers: usize) -> Self {
        self.max_workers = Some(max_workers);
        self
    }

    pub fn program_transformer_runner_sender(
        mut self,
        program_transformer_runner_sender: mpsc::Sender<Vec<AccountInfo>>,
    ) -> Self {
        self.program_transformer_runner_sender = Some(program_transformer_runner_sender);
        self
    }

    pub fn build(self) -> AccountSnapshotWriter {
        let channel_capacity = self.channel_capacity.expect("Channel capacity is required");
        let batch_size = self.batch_size.expect("Batch size is required");
        let pool = self.pool.expect("PgPool is required");
        let max_workers = self.max_workers.expect("Max workers is required");
        let program_transformer_runner_sender = self
            .program_transformer_runner_sender
            .expect("Program transformer runner sender is required");

        let (update_sender, mut update_receiver) = mpsc::channel::<AccountInfo>(channel_capacity);
        let (stop_sender, stop_receiver) = oneshot::channel::<()>();
        let (error_sender, error_receiver) = mpsc::channel::<()>(max_workers);

        let handle = tokio::spawn(async move {
            let mut updates = Vec::new();
            tokio::pin!(stop_receiver);
            let error_sender = error_sender.clone();
            let mut join_set = JoinSet::new();

            loop {
                tokio::select! {
                    Some(update) = update_receiver.recv() => {
                        updates.push(update);

                        if updates.len() >= batch_size {
                            let batch = std::mem::take(&mut updates);

                            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
                            let accounts: Vec<account_snapshots::ActiveModel> = batch
                                .clone()
                                .iter()
                                .map(|info| info.into_account_snapshot())
                                .collect();

                            while join_set.len() >= max_workers {
                                join_set.join_next().await;
                            }

                            let error_sender = error_sender.clone();

                            join_set.spawn(async move {
                                if (account_snapshots::Entity::insert_many(accounts)
                                    .exec(&conn)
                                    .await).is_err()
                                {
                                    if let Err(e) = error_sender.send(()).await {
                                        error!("Failed to send batch write error: {}", e);
                                    }
                                }
                            });

                            if let Err(e) = program_transformer_runner_sender.send(batch).await {
                                error!("Failed program transformer sender: {}", e)
                            }
                        }
                    }
                    _ = &mut stop_receiver => {
                        if !updates.is_empty() {
                            let batch = std::mem::take(&mut updates);

                            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
                            let accounts: Vec<account_snapshots::ActiveModel> = batch
                                .clone()
                                .iter()
                                .map(|info| info.into_account_snapshot())
                                .collect();

                            while join_set.len() >= max_workers {
                                join_set.join_next().await;
                            }


                            if (account_snapshots::Entity::insert_many(accounts)
                                .exec(&conn)
                                .await).is_err()
                            {
                                if let Err(e) = error_sender.send(()).await {
                                    error!("Failed to send batch write error: {}", e);
                                }
                            }

                            if let Err(e) = program_transformer_runner_sender.send(batch).await {
                                error!("Failed program transformer sender: {}", e)
                            }
                        }
                        break;
                    }
                }
            }

            while (join_set.join_next().await).is_some() {}
        });

        AccountSnapshotWriter {
            update_sender,
            stop_sender: Some(stop_sender),
            error_receiver: Some(error_receiver),
            handle,
        }
    }
}

impl AccountSnapshotWriter {
    pub fn builder() -> AccountSnapshotWriterBuilder {
        AccountSnapshotWriterBuilder::default()
    }

    pub fn sender(&self) -> mpsc::Sender<AccountInfo> {
        self.update_sender.clone()
    }

    pub fn take_error_receiver(&mut self) -> Option<mpsc::Receiver<()>> {
        self.error_receiver.take()
    }

    pub async fn shutdown(mut self) {
        if let Some(stop_sender) = self.stop_sender.take() {
            let _ = stop_sender.send(());
        }

        let _ = self.handle.await;
    }
}

pub struct ProgramTransformerRunner {
    handle: JoinHandle<()>,
    worker_sender: mpsc::Sender<Vec<AccountInfo>>,
    shutdown_sender: Option<oneshot::Sender<()>>,
}

impl ProgramTransformerRunner {
    pub fn builder() -> ProgramTransformerRunnerBuilder {
        ProgramTransformerRunnerBuilder::default()
    }

    pub fn sender(&self) -> mpsc::Sender<Vec<AccountInfo>> {
        self.worker_sender.clone()
    }

    pub async fn shutdown(mut self) {
        if let Some(shutdown_sender) = self.shutdown_sender.take() {
            let _ = shutdown_sender.send(());
        }

        let _ = self.handle.await;
    }
}

#[derive(Default)]
pub struct ProgramTransformerRunnerBuilder {
    max_workers: Option<usize>,
    buffer_capacity: Option<usize>,
    program_transformer: Option<Arc<ProgramTransformer>>,
}

impl ProgramTransformerRunnerBuilder {
    pub const fn max_workers(mut self, max_workers: usize) -> Self {
        self.max_workers = Some(max_workers);
        self
    }

    pub const fn buffer_capacity(mut self, buffer_capacity: usize) -> Self {
        self.buffer_capacity = Some(buffer_capacity);
        self
    }

    pub fn program_transformer(mut self, program_transformer: Arc<ProgramTransformer>) -> Self {
        self.program_transformer = Some(program_transformer);
        self
    }

    pub fn build(self) -> Result<ProgramTransformerRunner, anyhow::Error> {
        let buffer_capacity = self
            .buffer_capacity
            .unwrap_or(DEFAULT_PROGRAM_TRANSFORMER_BUFFER_CAPACITY);
        let (worker_sender, mut worker_receiver) =
            mpsc::channel::<Vec<AccountInfo>>(buffer_capacity);
        let (shutdown_sender, mut shutdown_receiver) = oneshot::channel();
        let program_transformer = self
            .program_transformer
            .expect("Program transform to be set");

        let max_workers = self
            .max_workers
            .unwrap_or(DEFAULT_PROGRAM_TRANSFORMER_MAX_WORKERS);

        let handle = tokio::spawn(async move {
            let mut join_set = JoinSet::new();

            loop {
                tokio::select! {
                    _ = &mut shutdown_receiver => {
                        break;
                    }
                    Some(account_infos) = worker_receiver.recv() => {
                        let program_transformer = Arc::clone(&program_transformer);

                        for account_info in account_infos {
                            while join_set.len() >= max_workers {
                                join_set.join_next().await;
                            }

                            let program_transformer = Arc::clone(&program_transformer);

                            join_set.spawn(async move {
                                let _ = program_transformer.handle_account_update(&account_info).await;
                            });
                        }
                    }
                }
            }

            while (join_set.join_next().await).is_some() {}
        });

        Ok(ProgramTransformerRunner {
            handle,
            worker_sender,
            shutdown_sender: Some(shutdown_sender),
        })
    }
}
