use crate::error::IngesterError;
use async_trait::async_trait;
use cadence_macros::statsd_count;
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use sqlx::{Pool, Postgres};
use std::fmt::Display;
use tokio::sync::mpsc::{self, UnboundedSender};

const NUM_TRIES: i32 = 5;

#[async_trait]
pub trait BgTask: Send + Sync + Display {
    fn name(&self) -> &'static str;
    async fn task(&self, db: &DatabaseConnection) -> Result<(), IngesterError>;
}

pub struct TaskManager {
    producer: UnboundedSender<Box<dyn BgTask>>,
}

impl TaskManager {
    pub fn new(_name: String, pool: Pool<Postgres>) -> Result<Self, IngesterError> {
        let (producer, mut receiver) = mpsc::unbounded_channel::<Box<dyn BgTask>>();

        tokio::spawn(async move {
            while let Some(data) = receiver.recv().await {
                let pool_cloned = pool.clone();
                let db = SqlxPostgresConnector::from_sqlx_postgres_pool(pool_cloned);
                // Spawning another task which allows us to catch panics.
                let task_res = tokio::spawn(async move {
                    for tries in 1..=NUM_TRIES {
                        match data.task(&db).await {
                            Ok(_) => {
                                statsd_count!("ingester.bgtask.complete", 1, "type" => data.name());
                                println!("{} completed, attempt {}", data, tries);
                                break;
                            }
                            Err(err) => {
                                statsd_count!("ingester.bgtask.error", 1, "type" => data.name());
                                println!("{} errored, attempt {}, with {:?}", data, tries, err);
                            }
                        }
                    }
                })
                .await;

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
            }
        });
        let tm = TaskManager { producer };
        Ok(tm)
    }

    pub fn get_sender(&self) -> UnboundedSender<Box<dyn BgTask>> {
        self.producer.clone()
    }
}
