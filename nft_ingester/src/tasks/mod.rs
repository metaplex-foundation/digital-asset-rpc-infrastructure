use std::collections::HashMap;
use crate::error::IngesterError;
use async_trait::async_trait;
use cadence_macros::{statsd_count, statsd_histogram};
use sea_orm::{DatabaseConnection,
              DatabaseTransaction,
              SqlxPostgresConnector,
              TransactionTrait,
              entity::*,
              query::*git
};
use sqlx::{Pool, Postgres};
use std::fmt::Display;
use std::sync::Arc;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use digital_asset_types::dao::{tasks};
use digital_asset_types::dao::sea_orm_active_enums::TaskStatus;
use sha2::{Sha256, Sha512, Digest};
use tokio::task::JoinHandle;

#[async_trait]
pub trait BgTask: Send + Sync + Display {
    fn name(&self) -> &'static str;
    fn data(&self) -> Result<serde_json::Value, IngesterError>;
    fn lock_duration(&self) -> i64;
    fn max_attempts(&self) -> i16;
    async fn task(&self, db: &DatabaseConnection) -> Result<(), IngesterError>;
}

pub struct TaskData {
    name: &'static str,
    data: serde_json::Value,
}

impl TaskData {
    pub fn hash(&self) -> Result<String, IngesterError> {
        let mut hasher = Sha256::new();
        if let Ok(data) = serde_json::to_vec(&self.data) {
            hasher.update(self.name.as_bytes());
            hasher.update(data.as_slice());
            return Ok(hasher.finalize()[..]);
        }
        Err(IngesterError::SerializatonError("Failed to serialize task data".to_string()))
    }
}

pub struct TaskManager {
    instance_name: String,
    pool: Pool<Postgres>,
    producer: UnboundedSender<TaskData>,
    receiver: Arc<UnboundedReceiver<TaskData>>,
    registered_task_types: HashMap<String, Box<dyn BgTask>>,
}

impl TaskManager {
    pub async fn get_pending_tasks(&self) -> Result<Vec<tasks::Model>, IngesterError> {
        tasks::Entity::find()
            .filter(tasks::Column::Status.eq(tasks::TaskStatus::Pending))
            .filter(tasks::Column::LockedUntil.ge(chrono::Utc::now()))
            .filter(tasks::Column::Attempts.le(tasks::Column::MaxAttempts))
            .all(&self.db)
            .await
    }

    pub async fn save_task(&self, task: &tasks::Model) -> Result<(), IngesterError> {
        task.save(&self.db).await?;
        Ok(())
    }

    pub async fn execute_task(txn: &DatabaseTransaction, task_def: &Box<dyn BgTask>, mut task: tasks::Model) -> Result<tasks::Model, IngesterError> {
        let task_name = task.r#type.clone();
        task.attempts = task.attempts + 1;
        let start = Utc::now();
        let res = task_executor.task(txn).await;
        let end = Utc::now();
        task.duration = Some(((end.timestamp_millis() - start.timestamp_millis()) / 1000 as i32) as i32);
        statsd_histogram!("ingester.bgtask.proc_time", end.timestamp_millis() - start.timestamp_millis(), "type" => data.name());
        match res {
            Ok(_) => {
                statsd_count!("ingester.bgtask.success", 1, "type" => task_name);
                task.status = TaskStatus::Success;
                task.errors = None;
                task.locked_until = None;
                task.locked_by = None;
            }
            Err(e) => {
                statsd_count!("ingester.bgtask.error", 1, "type" => task_name);
                task.status = TaskStatus::Failed;
                task.errors = Some(e.to_string());
                task.locked_until = None;
                task.locked_by = None;
            }
        }
        Ok(task)
    }

    pub fn new(instance_name: String, pool: Pool<Postgres>) -> Self {
        let (producer, mut receiver) = mpsc::unbounded_channel::<TaskData>();
        TaskManager {
            instance_name,
            pool,
            producer,
            receiver: Arc::new(receiver),
            registered_task_types: HashMap::new(),
        }
    }

    pub fn start(&self) -> JoinHandle<()> {
        let mut recv = self.receiver.clone();
        tokio::spawn(async move {
            while let Some(task) = recv.recv().await {
                let pool_cloned = self.pool.clone();
                let db = SqlxPostgresConnector::from_sqlx_postgres_pool(pool_cloned);
                if let Ok(hash) = task.hash() {
                    let task_entry = tasks::Entity::find_by_id(hash.clone())
                        .filter(tasks::Column::Status.ne(TaskStatus::Success))
                        .one(&db)
                        .await;
                    if task_entry.is_some() {
                        continue;
                    }
                    if let Some(task_executor) = self.registered_task_types.get(task.name) {
                        let task_res = tokio::spawn(async move {
                            let mut txn = db.begin().await?;
                            let lock = Utc::now() + Duration::seconds(task_executor.lock_duration());
                            let mut model = tasks::Model {
                                id: hash.clone(),
                                task_type: task.name.to_string(),
                                data: task.data,
                                status: TaskStatus::Pending,
                                created_at: Utc::now().naive_utc(),
                                locked_until: Some(lock.naive_utc()),
                                locked_by: Some(self.instance_name.clone()),
                                max_attempts: task_executor.max_attempts(),
                                attempts: 1,
                                duration: None,
                                errors: None,
                            };
                            let model = Self::execute_task(&txn, task_executor, model).await;
                            match model {
                                Ok(m) => {
                                    self.save_task(&m).await?;
                                    txn.commit().await
                                }
                                Err(e) => {
                                    txn.rollback().await
                                }
                            }?
                        }).await;

                        match task_res {
                            Ok(_) => (),
                            Err(err) if err.is_panic() => {
                                statsd_count!("ingester.bgtask.task_panic", 1);
                            }
                            Err(err) => {
                                let err = err.to_string();
                                statsd_count!("ingester.bgtask.task_error", 1, "error" => &err);
                            }
                        }
                    };
                }
            }
        })
    }

    pub fn get_sender(&self) -> UnboundedSender<TaskData> {
        self.producer.clone()
    }
}
