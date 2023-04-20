use crate::{error::IngesterError, metric};
use async_trait::async_trait;
use cadence_macros::{is_global_default_set, statsd_count, statsd_histogram};
use chrono::{Duration, NaiveDateTime, Utc};
use crypto::{digest::Digest, sha2::Sha256};
use digital_asset_types::dao::{sea_orm_active_enums::TaskStatus, tasks};
use log::{debug, error, warn};
use sea_orm::{
    entity::*, query::*, sea_query::Expr, ActiveValue::Set, ColumnTrait, DatabaseConnection,
    DeleteResult, SqlxPostgresConnector,
};
use sqlx::{Pool, Postgres};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::mpsc::{self, UnboundedSender},
    task::JoinHandle,
    time,
};
use serde::Deserialize;

mod common;
pub use common::*;

#[async_trait]
pub trait BgTask: Send + Sync {
    fn name(&self) -> &'static str;
    fn lock_duration(&self) -> i64;
    fn max_attempts(&self) -> i16;
    async fn task(
        &self,
        db: &DatabaseConnection,
        data: serde_json::Value,
    ) -> Result<(), IngesterError>;
}

pub const RETRY_INTERVAL: u64 = 1000;
pub const DELETE_INTERVAL: u64 = 30000;
pub const MAX_TASK_BATCH_SIZE: u64 = 100;
pub const PURGE_TIME: u64 = 3600;

/**
 * Configuration for the background task runner, to be used in config file loading e.g.
 */
#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct BackgroundTaskRunnerConfig {
    pub delete_interval: Option<u64>,
    pub retry_interval: Option<u64>,
    pub purge_time: Option<u64>,
    pub batch_size: Option<u64>,
    pub lock_duration: Option<i64>,
    pub max_attempts: Option<i16>,
    pub timeout: Option<u64>,
}

impl Default for BackgroundTaskRunnerConfig {
    fn default() -> Self {
        BackgroundTaskRunnerConfig {
            delete_interval: Some(DELETE_INTERVAL),
            retry_interval: Some(RETRY_INTERVAL),
            purge_time: Some(PURGE_TIME),
            batch_size: Some(MAX_TASK_BATCH_SIZE),
            lock_duration: Some(5),
            max_attempts: Some(3),
            timeout: Some(3),
        }
    }
}

pub struct TaskData {
    pub name: &'static str,
    pub data: serde_json::Value,
    pub created_at: Option<NaiveDateTime>,
}

impl TaskData {
    pub fn hash(&self) -> Result<String, IngesterError> {
        let mut hasher = Sha256::new();
        if let Ok(data) = serde_json::to_vec(&self.data) {
            hasher.input(self.name.as_bytes());
            hasher.input(data.as_slice());
            return Ok(hasher.result_str());
        }
        Err(IngesterError::SerializatonError(
            "Failed to serialize task data".to_string(),
        ))
    }
}

pub trait FromTaskData<T>: Sized {
    fn from_task_data(data: TaskData) -> Result<T, IngesterError>;
}

pub trait IntoTaskData: Sized {
    fn into_task_data(self) -> Result<TaskData, IngesterError>;
}

pub struct TaskManager {
    instance_name: String,
    pool: Pool<Postgres>,
    producer: Option<UnboundedSender<TaskData>>,
    registered_task_types: Arc<HashMap<String, Box<dyn BgTask>>>,
}

impl TaskManager {
    async fn execute_task(
        db: &DatabaseConnection,
        task_def: &Box<dyn BgTask>,
        mut task: tasks::ActiveModel,
    ) -> Result<tasks::ActiveModel, IngesterError> {
        let task_name = task_def.name();
        let attempts: Option<Value> = task.attempts.into_value();
        task.attempts = match attempts {
            Some(Value::SmallInt(Some(a))) => Set(a + 1),
            _ => Set(1),
        };
        let data_value: Option<Value> = task.data.clone().into_value();
        let data_json = match data_value {
            Some(Value::Json(Some(j))) => Ok(j),
            _ => Err(IngesterError::TaskManagerError(format!(
                "{} task data is not valid",
                task_name
            ))),
        }?;

        let start = Utc::now();
        let res = task_def.task(&db, *data_json).await;
        let end = Utc::now();
        task.duration = Set(Some(
            ((end.timestamp_millis() - start.timestamp_millis()) / 1000) as i32,
        ));
        metric! {
            statsd_histogram!("ingester.bgtask.proc_time", (end.timestamp_millis() - start.timestamp_millis()) as u64, "type" => task_name);
        }
        match res {
            Ok(_) => {
                metric! {
                    statsd_count!("ingester.bgtask.success", 1, "type" => task_name);
                }
                task.status = Set(TaskStatus::Success);
                task.errors = Set(None);
                task.locked_until = Set(None);
                task.locked_by = Set(None);
            }
            Err(e) => {
                if e == IngesterError::UnrecoverableTaskError {
                    task.attempts = Set(task_def.max_attempts() + 1);
                    task.locked_by = Set(Some("permanent failure".to_string()));
                // todo add new task status
                } else {
                    task.locked_by = Set(None);
                }
                task.status = Set(TaskStatus::Failed);
                task.errors = Set(Some(e.to_string()));
                task.locked_until = Set(None);

                match e {
                    IngesterError::BatchInitNetworkingError => {
                        // Network errors are common for off-chain JSONs.
                        // Logging these as errors is far too noisy.
                        metric! {
                            statsd_count!("ingester.bgtask.network_error", 1, "type" => task_name);
                        }
                        warn!("Task failed due to network error: {}",  e);
                    },
                    IngesterError::HttpError { ref status_code } => {
                        metric! {
                            statsd_count!("ingester.bgtask.http_error", 1, 
                                "status" => &status_code,
                                "type" => task_name);
                        }
                        warn!("Task failed due to HTTP error: {}",  e);
                    },
                    _ => {
                        metric! {
                            statsd_count!("ingester.bgtask.error", 1, "type" => task_name);
                        }
                        error!("Task Run Error: {}",  e);
                    }
                }
            }
        }
        Ok(task)
    }

    pub async fn get_pending_tasks(
        conn: &DatabaseConnection,
        batch_size: u64,
    ) -> Result<Vec<tasks::Model>, IngesterError> {
        tasks::Entity::find()
            .filter(
                Condition::all()
                    .add(tasks::Column::Status.ne(TaskStatus::Success))
                    .add(
                        Condition::any()
                            .add(tasks::Column::LockedUntil.lte(Utc::now()))
                            .add(tasks::Column::LockedUntil.is_null()),
                    )
                    .add(
                        Expr::col(tasks::Column::Attempts)
                            .less_or_equal(Expr::col(tasks::Column::MaxAttempts)),
                    ),
            )
            .order_by(tasks::Column::Attempts, Order::Asc)
            .order_by(tasks::Column::CreatedAt, Order::Desc)
            .limit(batch_size)
            .all(conn)
            .await
            .map_err(|e| e.into())
    }

    pub fn get_sender(&self) -> Result<UnboundedSender<TaskData>, IngesterError> {
        self.producer
            .clone()
            .ok_or(IngesterError::TaskManagerNotStarted)
    }

    fn lock_task(task: &mut tasks::ActiveModel, duration: Duration, instance_name: String) {
        task.status = Set(TaskStatus::Running);
        task.locked_until = Set(Some((Utc::now() + duration).naive_utc()));
        task.locked_by = Set(Some(instance_name));
    }

    pub fn new(
        instance_name: String,
        pool: Pool<Postgres>,
        task_defs: Vec<Box<dyn BgTask>>,
    ) -> Self {
        let mut tasks = HashMap::new();
        for task in task_defs {
            tasks.insert(task.name().to_string(), task);
        }
        TaskManager {
            instance_name,
            pool,
            producer: None,
            registered_task_types: Arc::new(tasks),
        }
    }

    fn new_task_handler(
        pool: Pool<Postgres>,
        instance_name: String,
        _name: String,
        task: TaskData,
        tasks_def: Arc<HashMap<String, Box<dyn BgTask>>>,
        process_now: bool,
    ) -> JoinHandle<Result<(), IngesterError>> {
        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
        tokio::task::spawn(async move {
            if let Some(task_executor) = tasks_def.get(task.name) {
                let mut model = tasks::ActiveModel {
                    id: Set(task.hash()?),
                    task_type: Set(task.name.to_string()),
                    data: Set(task.data),
                    status: Set(TaskStatus::Pending),
                    created_at: Set(Utc::now().naive_utc()),
                    locked_until: Set(None),
                    locked_by: Set(None),
                    max_attempts: Set(task_executor.max_attempts()),
                    attempts: Set(0),
                    duration: Set(None),
                    errors: Set(None),
                };
                let duration = Duration::seconds(task_executor.lock_duration());
                if process_now {
                    TaskManager::lock_task(&mut model, duration, instance_name);
                }
                let _model = model.insert(&conn).await?;
                Ok(())
            } else {
                Err(IngesterError::TaskManagerError(format!(
                    "{} not a valid task type",
                    task.name
                )))
            }
        })
    }

    pub async fn purge_old_tasks(conn: &DatabaseConnection, task_max_age: time::Duration) -> Result<DeleteResult, IngesterError> {
        let cod = Expr::cust_with_values("NOW() - created_at::timestamp > interval ? seconds", [task_max_age.as_secs()]); 
        tasks::Entity::delete_many()
            .filter(Condition::all().add(cod))
            .exec(conn)
            .await
            .map_err(|e| e.into())
    }

    async fn save_task<A>(
        txn: &A,
        task: tasks::ActiveModel,
    ) -> Result<tasks::ActiveModel, IngesterError>
    where
        A: ConnectionTrait,
    {
        let act: tasks::ActiveModel = task;
        act.save(txn).await.map_err(|e| e.into())
    }
    pub fn start_listener(&mut self, process_on_receive: bool) -> JoinHandle<()> {
        let (producer, mut receiver) = mpsc::unbounded_channel::<TaskData>();
        self.producer = Some(producer);
        let task_map = self.registered_task_types.clone();
        let pool = self.pool.clone();
        let instance_name = self.instance_name.clone();

        tokio::task::spawn(async move {
            while let Some(task) = receiver.recv().await {
                if let Some(task_created_time) = task.created_at {
                    let bus_time =
                        Utc::now().timestamp_millis() - task_created_time.timestamp_millis();
                    metric! {
                        statsd_histogram!("ingester.bgtask.bus_time", bus_time as u64, "type" => task.name);
                    }
                }
                let name = instance_name.clone();
                if let Ok(hash) = task.hash() {
                    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
                    let task_entry = tasks::Entity::find_by_id(hash.clone())
                        .filter(tasks::Column::Status.ne(TaskStatus::Pending))
                        .one(&conn)
                        .await;
                    if let Ok(Some(e)) = task_entry {
                        metric! {
                            statsd_count!("ingester.bgtask.identical", 1, "type" => &e.task_type);
                        }
                        continue;
                    }
                    metric! {
                        statsd_count!("ingester.bgtask.new", 1, "type" => &task.name);
                    }
                    TaskManager::new_task_handler(
                        pool.clone(),
                        instance_name.clone(),
                        name,
                        task,
                        task_map.clone(),
                        process_on_receive,
                    );
                }
            }
        })
    }

    pub fn start_runner(&self, config: Option<BackgroundTaskRunnerConfig>) -> JoinHandle<()> {
        let task_map = self.registered_task_types.clone();
        let instance_name = self.instance_name.clone();

        // Load the config values
        // For backwards compatibility reasons, the logic is a bit convoluted.
        let config = config.unwrap_or_default();

        let delete_interval = tokio::time::Duration::from_millis(
            config.delete_interval.unwrap_or(
                BackgroundTaskRunnerConfig::default().delete_interval.unwrap()
            ));

        let retry_interval = tokio::time::Duration::from_millis(
            config.retry_interval.unwrap_or(
                BackgroundTaskRunnerConfig::default().retry_interval.unwrap()));

        let purge_time = tokio::time::Duration::from_secs(
            config.purge_time.unwrap_or(
                BackgroundTaskRunnerConfig::default().purge_time.unwrap()));

        let batch_size = config.batch_size.unwrap_or(
            BackgroundTaskRunnerConfig::default().batch_size.unwrap());

        // Loop to purge tasks
        let pool = self.pool.clone();
        let task_name = instance_name.clone();
        tokio::spawn(async move {
            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
            let mut interval = time::interval(delete_interval);
            loop {
                interval.tick().await; // ticks immediately
                let delete_res = TaskManager::purge_old_tasks(&conn, purge_time).await;
                match delete_res {
                    Ok(res) => {
                        debug!("deleted {} tasks entries", res.rows_affected);
                        metric! {
                            statsd_count!("ingester.bgtask.purged_tasks", i64::try_from(res.rows_affected).unwrap_or(1));
                        }
                    }
                    Err(e) => {
                        metric! {
                            statsd_count!("ingester.bgtask.purge_error", 1);
                        }
                        error!("error deleting tasks: {}", e);
                    }
                };
            }
        });

        // Loop to check for tasks that need to be executed and execute them
        let pool = self.pool.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(retry_interval);
            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
            loop {
                interval.tick().await; // ticks immediately
                let tasks_res = TaskManager::get_pending_tasks(&conn, batch_size).await;
                match tasks_res {
                    Ok(tasks) => {
                        debug!("tasks that need to be executed: {}", tasks.len());
                        let _task_map_clone = task_map.clone();
                        let instance_name = instance_name.clone();
                        for task in tasks {
                            let task_map_clone = task_map.clone();
                            let instance_name_clone = instance_name.clone();
                            let pool = pool.clone();
                            tokio::task::spawn(async move {
                                if let Some(task_executor) =
                                    task_map_clone.clone().get(&*task.task_type)
                                {
                                    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
                                    let mut active_model: tasks::ActiveModel = task.into();
                                    TaskManager::lock_task(
                                        &mut active_model,
                                        Duration::seconds(task_executor.lock_duration()),
                                        instance_name_clone,
                                    );
                                    // can ignore as txn will bubble up errors
                                    let active_model =
                                        TaskManager::save_task(&conn, active_model).await?;
                                    let model = TaskManager::execute_task(
                                        &conn,
                                        task_executor,
                                        active_model,
                                    )
                                    .await?;
                                    TaskManager::save_task(&conn, model).await?;
                                    return Ok(());
                                }
                                Err(IngesterError::TaskManagerError(format!(
                                    "{} not a valid task type",
                                    task.task_type
                                )))
                            });
                        }
                    }
                    Err(e) => {
                        error!("Error getting pending tasks: {}", e);
                    }
                }
            }
        })
    }
}
