use {
    crate::{
        config::{ConfigDownloadMetadata, ConfigDownloadMetadataOpts},
        postgres::{create_pool as pg_create_pool, metrics_pgpool},
        util::create_shutdown,
    },
    das_core::DownloadMetadataInfo,
    digital_asset_types::dao::{asset_data, sea_orm_active_enums::TaskStatus, tasks},
    futures::{
        future::{pending, FutureExt},
        stream::StreamExt,
    },
    reqwest::{ClientBuilder, StatusCode},
    sea_orm::{
        entity::{ActiveValue, ColumnTrait, EntityTrait},
        query::{Condition, Order, QueryFilter, QueryOrder, QuerySelect},
        sea_query::expr::Expr,
        SqlxPostgresConnector, TransactionTrait,
    },
    sqlx::PgPool,
    std::{sync::Arc, time::Duration},
    tokio::{
        sync::{mpsc, Notify},
        task::JoinSet,
        time::sleep,
    },
    tracing::{info, warn},
};

pub const TASK_TYPE: &str = "DownloadMetadata";

pub async fn run(config: ConfigDownloadMetadata) -> anyhow::Result<()> {
    let mut shutdown = create_shutdown()?;

    // open connection to postgres
    let pool = pg_create_pool(config.postgres).await?;
    tokio::spawn({
        let pool = pool.clone();
        async move { metrics_pgpool(pool).await }
    });

    // reset previously runned tasks
    tokio::select! {
        result = reset_pending_tasks(pool.clone()) => {
            let updated = result?;
            info!("Reset {updated} tasks to Pending status");
        },
        Some(signal) = shutdown.next() => {
            warn!("{signal} received, waiting spawned tasks...");
            return Ok(())
        },
    }

    // prefetch queue
    let (tasks_tx, mut tasks_rx) = mpsc::channel(config.download_metadata.prefetch_queue_size);
    let prefetch_shutdown = Arc::new(Notify::new());
    let prefetch_jh = {
        let pool = pool.clone();
        let download_metadata = config.download_metadata;
        let shutdown = Arc::clone(&prefetch_shutdown);
        async move {
            tokio::select! {
                result = get_pending_tasks(pool, tasks_tx, download_metadata) => result,
                _ = shutdown.notified() => Ok(())
            }
        }
    };
    tokio::pin!(prefetch_jh);

    // process tasks
    let mut tasks = JoinSet::new();
    loop {
        let pending_task_fut = if tasks.len() >= config.download_metadata.max_in_process {
            pending().boxed()
        } else {
            tasks_rx.recv().boxed()
        };

        let tasks_fut = if tasks.is_empty() {
            pending().boxed()
        } else {
            tasks.join_next().boxed()
        };

        tokio::select! {
            Some(signal) = shutdown.next() => {
                warn!("{signal} received, waiting spawned tasks...");
                break Ok(());
            },
            result = &mut prefetch_jh => break result,
            Some(result) = tasks_fut => {
                result??;
            },
            Some(pending_task) = pending_task_fut => {
                tasks.spawn(execute_task(pool.clone(), pending_task, config.download_metadata.download_timeout));
            }
        };
    }?;

    tokio::select! {
        Some(signal) = shutdown.next() => {
            anyhow::bail!("{signal} received, force shutdown...");
        }
        result = async move {
            // shutdown `prefetch` channel
            prefetch_shutdown.notify_one();
            // wait all spawned tasks
            while let Some(result) = tasks.join_next().await {
                result??;
            }
            // shutdown database connection
            pool.close().await;
            Ok::<(), anyhow::Error>(())
        } => result,
    }
}

// On startup reset tasks status
async fn reset_pending_tasks(pool: PgPool) -> anyhow::Result<u64> {
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
    tasks::Entity::update_many()
        .set(tasks::ActiveModel {
            status: ActiveValue::Set(TaskStatus::Pending),
            ..Default::default()
        })
        .filter(
            Condition::all()
                .add(tasks::Column::Status.eq(TaskStatus::Running))
                .add(tasks::Column::TaskType.eq(TASK_TYPE)),
        )
        .exec(&conn)
        .await
        .map(|result| result.rows_affected)
        .map_err(Into::into)
}

// Select Pending tasks, update status to Running and send to prefetch queue
async fn get_pending_tasks(
    pool: PgPool,
    tasks_tx: mpsc::Sender<tasks::Model>,
    config: ConfigDownloadMetadataOpts,
) -> anyhow::Result<()> {
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
    loop {
        let pending_tasks = tasks::Entity::find()
            .filter(
                Condition::all()
                    .add(tasks::Column::Status.eq(TaskStatus::Pending))
                    .add(
                        Expr::col(tasks::Column::Attempts)
                            .less_or_equal(Expr::col(tasks::Column::MaxAttempts)),
                    ),
            )
            .order_by(tasks::Column::Attempts, Order::Asc)
            .order_by(tasks::Column::CreatedAt, Order::Desc)
            .limit(config.limit_to_fetch as u64)
            .all(&conn)
            .await?;

        if pending_tasks.is_empty() {
            sleep(config.wait_tasks_max_idle).await;
        } else {
            tasks::Entity::update_many()
                .set(tasks::ActiveModel {
                    status: ActiveValue::Set(TaskStatus::Running),
                    ..Default::default()
                })
                .filter(tasks::Column::Id.is_in(pending_tasks.iter().map(|v| v.id.clone())))
                .exec(&conn)
                .await?;

            for task in pending_tasks {
                tasks_tx
                    .send(task)
                    .await
                    .map_err(|_error| anyhow::anyhow!("failed to send task to prefetch queue"))?;
            }
        }
    }
}

// Try to download metadata and remove task with asset_data update or update tasks to Pending/Failed
async fn execute_task(pool: PgPool, task: tasks::Model, timeout: Duration) -> anyhow::Result<()> {
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
    match download_metadata(task.data, timeout).await {
        Ok((asset_data_id, metadata)) => {
            // Remove task and set metadata in transacstion
            let txn = conn.begin().await?;
            tasks::Entity::delete_by_id(task.id).exec(&txn).await?;
            asset_data::Entity::update(asset_data::ActiveModel {
                id: ActiveValue::Unchanged(asset_data_id),
                metadata: ActiveValue::Set(metadata),
                reindex: ActiveValue::Set(Some(false)),
                ..Default::default()
            })
            .exec(&txn)
            .await?;
            txn.commit().await?;
        }
        Err(error) => {
            let status = if task.attempts + 1 == task.max_attempts {
                TaskStatus::Failed
            } else {
                TaskStatus::Pending
            };
            tasks::Entity::update(tasks::ActiveModel {
                id: ActiveValue::Unchanged(task.id),
                status: ActiveValue::Set(status),
                attempts: ActiveValue::Set(task.attempts + 1),
                errors: ActiveValue::Set(Some(error.to_string())),
                ..Default::default()
            })
            .exec(&conn)
            .await?;
        }
    }
    Ok(())
}

async fn download_metadata(
    data: serde_json::Value,
    timeout: Duration,
) -> anyhow::Result<(Vec<u8>, serde_json::Value)> {
    let (id, uri, _slot) = serde_json::from_value::<DownloadMetadataInfo>(data)?.into_inner();

    // Need to check for malicious sites ?
    let client = ClientBuilder::new().timeout(timeout).build()?;
    let response = client.get(uri).send().await?;

    anyhow::ensure!(
        response.status() == StatusCode::OK,
        "HttpError status_code: {}",
        response.status().as_str()
    );
    Ok((id, response.json().await?))
}
