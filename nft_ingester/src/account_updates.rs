use std::{sync::Arc, collections::{HashSet, HashMap}};

use crate::{
    error::IngesterError, metric, program_transformers::ProgramTransformer, tasks::TaskData, config::rand_string,
};
use cadence_macros::{is_global_default_set, statsd_count, statsd_gauge, statsd_time};
use chrono::Utc;

use figment::value::Value;
use log::{debug, error, info};
use plerkle_messenger::{ConsumptionType, Messenger, MessengerConfig, RecvData};
use plerkle_serialization::root_as_account_info;
use sqlx::{Pool, Postgres};
use tokio::{
    sync::mpsc::UnboundedSender,
    task::{JoinHandle, JoinSet},
    time::Instant,
};

pub fn account_worker<T: Messenger>(
    pool: Pool<Postgres>,
    stream: &'static str,
    config: MessengerConfig,
    bg_task_sender: UnboundedSender<TaskData>,
    ack_channel: UnboundedSender<String>,
    consumption_type: ConsumptionType,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let source = T::new(config).await;
        if let Ok(mut msg) = source {
            let manager = Arc::new(ProgramTransformer::new(pool, bg_task_sender));
            loop {
                let e = msg.recv(&stream, consumption_type.clone()).await;
                match e {
                    Ok(data) => {
                        let mut tasks = JoinSet::new();
                        for item in data {
                            tasks.spawn(handle_account(Arc::clone(&manager), item));
                        }
                        while let Some(res) = tasks.join_next().await {
                            if let Ok(Some(id)) = res {
                                let send = ack_channel.send(id);
                                if let Err(err) = send {
                                    metric! {
                                        error!("Account stream ack error: {}", err);
                                        statsd_count!("ingester.stream.ack_error", 1, "stream" => stream);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving from account stream: {}", e);
                        metric! {
                            statsd_count!("ingester.stream.receive_error", 1, "stream" => stream);
                        }
                    }
                }
            }
        }
    })
}

async fn handle_account(manager: Arc<ProgramTransformer>, item: RecvData) -> Option<String> {
    let id = item.id;
    let mut ret_id = None;
    if item.tries > 0 {
        metric! {
            statsd_count!("ingester.account_stream_redelivery", 1);
        }
    }
    let data = item.data;
    // Get root of account info flatbuffers object.
    if let Ok(account_update) = root_as_account_info(&data) {
        let seen_at = Utc::now();
        let str_program_id =
            bs58::encode(account_update.owner().unwrap().0.as_slice()).into_string();
        metric! {
            statsd_count!("ingester.account_update_seen", 1, "owner" => &str_program_id);
            statsd_time!(
                "ingester.account_bus_ingest_time",
                (seen_at.timestamp_millis() - account_update.seen_at()) as u64,
                "owner" => &str_program_id
            );
        }
        let begin_processing = Instant::now();
        let res = manager.handle_account_update(account_update).await;
        match res {
            Ok(_) => {
                if item.tries == 0 {
                    metric! {
                        statsd_time!("ingester.account_proc_time", begin_processing.elapsed().as_millis() as u64, "owner" => &str_program_id);
                        statsd_count!("ingester.account_update_success", 1, "owner" => &str_program_id);
                    }
                } else {
                    metric! {
                        statsd_count!("ingester.account_ingest_redeliver_success", 1);
                    }
                }
                ret_id = Some(id);
            }
            Err(err) if err == IngesterError::NotImplemented => {
                metric! {
                    statsd_count!("ingester.account_not_implemented", 1, "owner" => &str_program_id, "error" => "ni");
                }
                ret_id = Some(id);
            }
            Err(IngesterError::DeserializationError(e)) => {
                metric! {
                    statsd_count!("ingester.account_update_error", 1, "owner" => &str_program_id, "error" => "de");
                }
                error!("{}", e);
                ret_id = Some(id);
            }
            Err(IngesterError::ParsingError(e)) => {
                metric! {
                    statsd_count!("ingester.account_update_error", 1, "owner" => &str_program_id, "error" => "parse");
                }
                error!("{}", e);
                ret_id = Some(id);
            }
            Err(err) => {
                println!("Error handling account update: {:?}", err);
                metric! {
                    statsd_count!("ingester.account_update_error", 1, "owner" => &str_program_id, "error" => "u");
                }
            }
        }
    }
    ret_id
}
