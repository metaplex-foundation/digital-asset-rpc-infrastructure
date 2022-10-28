use std::collections::HashMap;
use crate::error::IngesterError;
use async_trait::async_trait;
use cadence_macros::{statsd_count, statsd_histogram};
use sea_orm::{DatabaseConnection,
              DatabaseTransaction,
              SqlxPostgresConnector,
              TransactionTrait,
              entity::*,
              ColumnTrait,
              query::*,
};
use sqlx::{Pool, Postgres};
use std::fmt::Display;
use std::sync::Arc;
use chrono::{Duration, Utc};
use crypto::sha2::Sha256;
use crypto::digest::Digest;
use sea_orm::sea_query::{ConditionExpression, Expr, SimpleExpr};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use digital_asset_types::dao::{tasks};
use digital_asset_types::dao::sea_orm_active_enums::TaskStatus;
use tokio::task::{JoinError, JoinHandle};
use tokio::{time};

#[async_trait]
pub trait BgTask: Send + Sync + Display {
    fn name(&self) -> &'static str;
    fn data(&self) -> Result<serde_json::Value, IngesterError>;
    fn lock_duration(&self) -> i64;
    fn max_attempts(&self) -> i16;
    async fn task(&self, db: &DatabaseTransaction) -> Result<(), IngesterError>;
}

pub struct TaskData {
    name: &'static str,
    data: serde_json::Value,
}

impl TaskData {
    pub fn hash(&self) -> Result<String, IngesterError> {
        let mut hasher = Sha256::new();
        if let Ok(data) = serde_json::to_vec(&self.data) {
            hasher.input(self.name.as_bytes());
            hasher.input(&data.as_slice());
            return Ok(hasher.result_str());
        }
        Err(IngesterError::SerializatonError("Failed to serialize task data".to_string()))
    }
}

pub trait IntoTaskData: BgTask + Sized {
    fn into_task_data(self) -> Result<TaskData, IngesterError> {
        self.data().map(|data| TaskData {
            name: self.name(),
            data,
        })
    }
}

pub struct TaskManager {
    instance_name: String,
    pool: Pool<Postgres>,
    producer: Option<UnboundedSender<TaskData>>,
    registered_task_types: Arc<HashMap<String, Box<dyn BgTask>>>,
}

impl TaskManager {
    pub async fn get_pending_tasks(conn: &DatabaseConnection) -> Result<Vec<tasks::Model>, IngesterError> {
        tasks::Entity::find()
            .filter(Condition::all()
                .add(tasks::Column::Status.ne(TaskStatus::Success))
                .add(tasks::Column::Status.ne(TaskStatus::Running))
                .add(tasks::Column::LockedUntil.lte(Utc::now()))
                .add(Expr::col(tasks::Column::Attempts).less_or_equal(Expr::col(tasks::Column::MaxAttempts)))
            )
            .all(conn)
            .await
            .map_err(|e| e.into())
    }

    async fn save_task<A>(txn: &A, task: tasks::Model) -> Result<tasks::Model, IngesterError>
        where
            A: ConnectionTrait
    {
        task.clone().into_active_model().save(txn).await?;
        Ok(task)
    }

    fn lock_task(task: &mut tasks::Model, duration: Duration, instance_name: String) {
        task.status = TaskStatus::Running;
        task.locked_until = Some((Utc::now() + duration).naive_utc());
        task.locked_by = Some(instance_name);
    }

    fn new_task_executor(pool: Pool<Postgres>, name: String, task: TaskData, tasks_def: Arc<HashMap<String, Box<dyn BgTask>>>) -> JoinHandle<Result<(), IngesterError>> {
        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
        tokio::spawn(async move {
            if let Some(task_executor) = tasks_def.get(task.name) {
                let txn = conn.begin().await?;
                let mut model = tasks::Model {
                    id: task.hash()?,
                    task_type: task.name.to_string(),
                    data: task.data,
                    status: TaskStatus::Pending,
                    created_at: Utc::now().naive_utc(),
                    locked_until: None,
                    locked_by: None,
                    max_attempts: task_executor.max_attempts(),
                    attempts: 1,
                    duration: None,
                    errors: None,
                };
                TaskManager::lock_task(&mut model, Duration::seconds(task_executor.lock_duration()), name);
                let model = TaskManager::execute_task(&txn, task_executor, model).await;
                match model {
                    Ok(m) => {
                        TaskManager::save_task(&txn, m).await?;
                        txn.commit().await
                    }
                    Err(e) => {
                        txn.rollback().await
                    }
                }.map_err(|e| e.into())
            } else {
                Err(IngesterError::TaskManagerError(format!("{} not a valid task type", task.name.to_string())))
            }
        })
    }

    async fn execute_task(txn: &DatabaseTransaction, task_def: &Box<dyn BgTask>, mut task: tasks::Model) -> Result<tasks::Model, IngesterError> {
        let task_name = task.task_type.clone();
        task.attempts = task.attempts + 1;
        let start = Utc::now();
        let res = task_def.task(txn).await;
        let end = Utc::now();
        task.duration = Some(((end.timestamp_millis() - start.timestamp_millis()) / 1000) as i32);
        statsd_histogram!("ingester.bgtask.proc_time", (end.timestamp_millis() - start.timestamp_millis()) as u64, "type" => &task_name);
        match res {
            Ok(_) => {
                statsd_count!("ingester.bgtask.success", 1, "type" => &task_name);
                task.status = TaskStatus::Success;
                task.errors = None;
                task.locked_until = None;
                task.locked_by = None;
            }
            Err(e) => {
                statsd_count!("ingester.bgtask.error", 1, "type" => &task_name);
                task.status = TaskStatus::Failed;
                task.errors = Some(e.to_string());
                task.locked_until = None;
                task.locked_by = None;
            }
        }
        Ok(task)
    }

    pub fn new(instance_name: String, pool: Pool<Postgres>) -> Self {
        TaskManager {
            instance_name,
            pool,
            producer: None,
            registered_task_types: Arc::new(HashMap::new()),
        }
    }

    fn task_metrics(task_res: Result<Result<(), IngesterError>, JoinError>) {
        match task_res {
            Ok(Ok(_)) => (),
            Ok(Err(_)) => {
                statsd_count!("ingester.bgtask.task_error", 1); // we dont send the error string to metrics system as that would blow up the radix
            }
            Err(err) if err.is_panic() => {
                statsd_count!("ingester.bgtask.task_panic", 1);
            }
            Err(_) => {
                statsd_count!("ingester.bgtask.task_error", 1);
            }
        };
    }

    pub fn start(&mut self) -> (JoinHandle<()>, JoinHandle<()>) {
        let (producer, mut receiver) = mpsc::unbounded_channel::<TaskData>();
        self.producer = Some(producer);
        let task_map = self.registered_task_types.clone();
        let pool = self.pool.clone();
        let instance_name = self.instance_name.clone();


        let listener_handle = tokio::spawn(async move {
            while let Some(task) = receiver.recv().await {
                let name = instance_name.clone();
                if let Ok(hash) = task.hash() {
                    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
                    let task_entry = tasks::Entity::find_by_id(hash.clone())
                        .filter(tasks::Column::Status.ne(TaskStatus::Success))
                        .one(&conn)
                        .await;
                    if let Ok(Some(e)) = task_entry {
                        statsd_count!("ingester.bgtask.identical", 1, "type" => &e.task_type);
                        continue;
                    }
                    let task_res = TaskManager::new_task_executor(
                        pool.clone(),
                        name,
                        task,
                        task_map.clone(),
                    ).await;
                    TaskManager::task_metrics(task_res);
                };
            }
        });

        let task_map = self.registered_task_types.clone();
        let pool = self.pool.clone();
        let instance_name = self.instance_name.clone();
        let scheduler_handle = tokio::spawn(async move {
            let mut interval = time::interval(tokio::time::Duration::from_millis(10));
            loop {
                interval.tick().await; // ticks immediately
                let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
                let tasks_res = TaskManager::get_pending_tasks(&conn).await;
                match tasks_res {
                    Ok(tasks) => {
                        for mut task in tasks {
                            let task_map_clone = task_map.clone();
                            let instance_name = instance_name.clone();
                            let pool = pool.clone();
                            let task_res = tokio::task::spawn(async move {
                                if let Some(task_executor) = task_map_clone.clone().get(&*task.task_type) {
                                    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
                                    TaskManager::lock_task(&mut task, Duration::seconds(task_executor.lock_duration()), instance_name);
                                    let task = TaskManager::save_task(&conn, task).await?;
                                    let mut txn = conn.begin().await?;
                                    let model = TaskManager::execute_task(&txn, task_executor, task).await;
                                    return match model {
                                        Ok(m) => {
                                            TaskManager::save_task(&txn, m).await?;
                                            txn.commit().await
                                        }
                                        Err(e) => {
                                            txn.rollback().await
                                        }
                                    }.map_err(|e| e.into());
                                }
                                Err(IngesterError::TaskManagerError(format!("{} not a valid task type", task.task_type)))
                            }).await;
                            TaskManager::task_metrics(task_res);
                        }
                    }
                    Err(e) => {
                        println!("Error getting pending tasks: {}", e.to_string());
                    }
                }
            }
        });

        (scheduler_handle, listener_handle)
    }


    pub fn get_sender(&self) -> Result<UnboundedSender<TaskData>, IngesterError> {
        self.producer.clone().ok_or(IngesterError::TaskManagerNotStarted)
    }
}
