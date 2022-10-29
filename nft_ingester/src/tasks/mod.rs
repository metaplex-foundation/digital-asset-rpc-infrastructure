use crate::{error::IngesterError, safe_metric};
use async_trait::async_trait;
use cadence_macros::{statsd_count, statsd_histogram};
use chrono::{Duration, Utc};
use crypto::{digest::Digest, sha2::Sha256};
use digital_asset_types::dao::{sea_orm_active_enums::TaskStatus, tasks};
use sea_orm::{
    entity::*,
    query::*,
    sea_query::{Expr},
    ActiveValue::Set,
    ColumnTrait, DatabaseConnection, DatabaseTransaction, DeleteResult, SqlxPostgresConnector,
    TransactionTrait,
};

use sqlx::{Pool, Postgres};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::mpsc::{self, UnboundedSender},
    task::{JoinError, JoinHandle},
    time,
};

pub mod common;

#[async_trait]
pub trait BgTask: Send + Sync {
    fn name(&self) -> &'static str;
    fn lock_duration(&self) -> i64;
    fn max_attempts(&self) -> i16;
    async fn task(
        &self,
        db: &DatabaseTransaction,
        data: serde_json::Value,
    ) -> Result<(), IngesterError>;
}
const RETRY_INTERVAL: u64 = 10000;
const MAX_TASK_BATCH_SIZE: u64 = 200;
pub struct TaskData {
    pub name: &'static str,
    pub data: serde_json::Value,
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
    pub async fn get_pending_tasks(
        conn: &DatabaseConnection,
    ) -> Result<Vec<tasks::Model>, IngesterError> {
        tasks::Entity::find()
            .filter(
                Condition::all()
                    .add(tasks::Column::Status.ne(TaskStatus::Success))
                    .add(tasks::Column::Status.ne(TaskStatus::Running))
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
            .limit(MAX_TASK_BATCH_SIZE)
            .all(conn)
            .await
            .map_err(|e| e.into())
    }

    pub async fn purge_old_tasks(conn: &DatabaseConnection) -> Result<DeleteResult, IngesterError> {
        let cod = Expr::cust("NOW() - created_at::timestamp > interval '1 minute'"); //TOdo parametrize
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

    fn lock_task(task: &mut tasks::ActiveModel, duration: Duration, instance_name: String) {
        task.status = Set(TaskStatus::Running);
        task.locked_until = Set(Some((Utc::now() + duration).naive_utc()));
        task.locked_by = Set(Some(instance_name));
    }

    fn new_task_executor(
        pool: Pool<Postgres>,
        name: String,
        task: TaskData,
        tasks_def: Arc<HashMap<String, Box<dyn BgTask>>>,
    ) -> JoinHandle<Result<(), IngesterError>> {
        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
        tokio::spawn(async move {
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
                TaskManager::lock_task(
                    &mut model,
                    Duration::seconds(task_executor.lock_duration()),
                    name,
                );
                let model = model.insert(&conn).await?;

                let txn = conn.begin().await?;
                let model = TaskManager::execute_task(&txn, task_executor, model.into()).await;
                match model {
                    Ok(m) => {
                        TaskManager::save_task(&txn, m).await?;
                        txn.commit().await
                    }
                    Err(_e) => txn.rollback().await,
                }
                .map_err(|e| e.into())
            } else {
                Err(IngesterError::TaskManagerError(format!(
                    "{} not a valid task type",
                    task.name
                )))
            }
        })
    }

    async fn execute_task(
        txn: &DatabaseTransaction,
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
        let res = task_def.task(txn, *data_json).await;
        let end = Utc::now();
        task.duration = Set(Some(
            ((end.timestamp_millis() - start.timestamp_millis()) / 1000) as i32,
        ));
        safe_metric(|| {
            statsd_histogram!("ingester.bgtask.proc_time", (end.timestamp_millis() - start.timestamp_millis()) as u64, "type" => task_name);
        });
        match res {
            Ok(_) => {
                safe_metric(|| {
                    statsd_count!("ingester.bgtask.success", 1, "type" => task_name);
                });
                task.status = Set(TaskStatus::Success);
                task.errors = Set(None);
                task.locked_until = Set(None);
                task.locked_by = Set(None);
            }
            Err(e) => {
                safe_metric(|| {
                    statsd_count!("ingester.bgtask.error", 1, "type" => task_name);
                });
                task.status = Set(TaskStatus::Failed);
                task.errors = Set(Some(e.to_string()));
                task.locked_until = Set(None);
                task.locked_by = Set(None);
            }
        }
        Ok(task)
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

    fn task_metrics(task_res: Result<Result<(), IngesterError>, JoinError>) {
        match task_res {
            Ok(Ok(_)) => (),
            Ok(Err(e)) => {
                println!("new task error: {}", e);
            }
            Err(err) if err.is_panic() => {
                safe_metric(|| {
                    statsd_count!("ingester.bgtask.task_panic", 1);
                });
            }
            Err(_) => (),
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
                        .filter(tasks::Column::Status.ne(TaskStatus::Pending))
                        .one(&conn)
                        .await;
                    if let Ok(Some(e)) = task_entry {
                        safe_metric(|| {
                            statsd_count!("ingester.bgtask.identical", 1, "type" => &e.task_type);
                        });
                        continue;
                    }
                    let task_res =
                        TaskManager::new_task_executor(pool.clone(), name, task, task_map.clone())
                            .await;
                    TaskManager::task_metrics(task_res);
                };
            }
        });

        let task_map = self.registered_task_types.clone();
        let pool = self.pool.clone();
        let instance_name = self.instance_name.clone();
        let scheduler_handle = tokio::spawn(async move {
            let mut interval = time::interval(tokio::time::Duration::from_millis(RETRY_INTERVAL));
            loop {
                interval.tick().await; // ticks immediately
                let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
                let delete_res = TaskManager::purge_old_tasks(&conn).await;
                match delete_res {
                    Ok(res) => {
                        println!("deleted {} tasks entries", res.rows_affected);
                    }
                    Err(e) => {
                        println!("error deleting tasks: {}", e);
                    }
                };
                let tasks_res = TaskManager::get_pending_tasks(&conn).await;
                match tasks_res {
                    Ok(tasks) => {
                        println!("tasks that need to be executed: {}", tasks.len());
                        let task_map_clone = task_map.clone();
                        let instance_name = instance_name.clone();
                        if let Ok(txn) = conn.begin().await {
                            for task in tasks.iter() {
                                if let Some(task_executor) =
                                    task_map_clone.clone().get(&*task.task_type)
                                {
                                    let mut active_model: tasks::ActiveModel = task.clone().into(); //requires owned
                                    TaskManager::lock_task(
                                        &mut active_model,
                                        Duration::seconds(task_executor.lock_duration()),
                                        instance_name.clone(),
                                    );
                                    // can ignore as txn will bubble up errors
                                    TaskManager::save_task(&txn, active_model).await;
                                }
                            }
                            txn.commit().await;
                        }
                        for task in tasks {
                            let task_map_clone = task_map.clone();
                            let _instance_name = instance_name.clone();
                            let pool = pool.clone();
                            let task_res = tokio::task::spawn(async move {
                                if let Some(task_executor) =
                                    task_map_clone.clone().get(&*task.task_type)
                                {
                                    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
                                    let active_model: tasks::ActiveModel = task.into();
                                    let txn = conn.begin().await?;
                                    let model = TaskManager::execute_task(
                                        &txn,
                                        task_executor,
                                        active_model,
                                    )
                                    .await;
                                    return match model {
                                        Ok(m) => {
                                            TaskManager::save_task(&txn, m).await?;
                                            txn.commit().await
                                        }
                                        Err(_e) => txn.rollback().await,
                                    }
                                    .map_err(|e| e.into());
                                }
                                Err(IngesterError::TaskManagerError(format!(
                                    "{} not a valid task type",
                                    task.task_type
                                )))
                            })
                            .await;
                            TaskManager::task_metrics(task_res);
                        }
                    }
                    Err(e) => {
                        println!("Error getting pending tasks: {}", e);
                    }
                }
            }
        });

        (scheduler_handle, listener_handle)
    }

    pub fn get_sender(&self) -> Result<UnboundedSender<TaskData>, IngesterError> {
        self.producer
            .clone()
            .ok_or(IngesterError::TaskManagerNotStarted)
    }
}
