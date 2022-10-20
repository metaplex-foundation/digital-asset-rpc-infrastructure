use crate::error::IngesterError;
use async_trait::async_trait;
use cadence_macros::statsd_count;
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use sqlx::{Pool, Postgres};
use std::fmt::Display;
use tokio::{
    runtime::{Builder, Runtime},
    sync::mpsc::{self, UnboundedSender},
};

#[async_trait]
pub trait BgTask: Send + Sync + Display {
    fn name(&self) -> &'static str;
    async fn task(&self, db: &DatabaseConnection) -> Result<(), IngesterError>;
}

pub struct TaskManager {
    runtime: Runtime,
    producer: UnboundedSender<Box<dyn BgTask>>,
}

impl TaskManager {
    pub fn new(name: String, pool: Pool<Postgres>) -> Result<Self, IngesterError> {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .thread_name(name)
            .build()
            .map_err(|err| {
                IngesterError::TaskManagerError(format!(
                    "Could not create tokio runtime: {:?}",
                    err
                ))
            })?;

        let (producer, mut receiver) = mpsc::unbounded_channel::<Box<dyn BgTask>>();
        let db = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
        runtime.spawn(async move {
            while let Some(data) = receiver.recv().await {
                let task_res = data.task(&db).await;

                match task_res {
                    Ok(_) => {
                        statsd_count!("ingester.bgtask.complete", 1, "type" => data.name());
                        println!("{} completed", data)
                    }
                    Err(e) => {
                        statsd_count!("ingester.bgtask.error", 1, "type" => data.name());
                        println!("{} errored with {:?}", data, e)
                    }
                }
            }
        });
        let tm = TaskManager { runtime, producer };
        Ok(tm)
    }

    pub fn get_sender(&self) -> UnboundedSender<Box<dyn BgTask>> {
        self.producer.clone()
    }
}
