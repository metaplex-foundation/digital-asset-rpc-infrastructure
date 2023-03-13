use std::sync::Arc;

use crate::{
    error::IngesterError, metric, program_transformers::ProgramTransformer,
    stream::MessengerDataStream, tasks::TaskData,
};
use cadence_macros::{is_global_default_set, statsd_count, statsd_time};
use chrono::Utc;
use futures::StreamExt;
use plerkle_messenger::RecvData;
use plerkle_serialization::root_as_account_info;
use sqlx::{Pool, Postgres};
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle, time::Instant};

pub fn setup_account_stream_worker(
    pool: Pool<Postgres>,
    bg_task_sender: UnboundedSender<TaskData>,
    mut stream: MessengerDataStream,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let manager = Arc::new(ProgramTransformer::new(pool, bg_task_sender));
        let acker = stream.ack_sender();
        let mut last_ack = Instant::now();
        let mut acks = Vec::new();
        while let Some(item) = stream.next().await {
            if let Some(id) = handle_account(&manager, item).await {
                acks.push(id);
                if last_ack.elapsed().as_secs() > 1 || acks.len() > 100 {
                    let mut send_acks = Vec::with_capacity(acks.len());
                    send_acks.append(&mut acks);
                    acker.send(send_acks).unwrap();
                    last_ack = Instant::now();
                }
            }
        }
    })
}

#[inline(always)]
async fn handle_account(manager: &Arc<ProgramTransformer>, item: RecvData) -> Option<String> {
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
                    }
                    metric! {
                        statsd_count!("ingester.account_update_success", 1, "owner" => &str_program_id);
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
            Err(IngesterError::DeserializationError(_)) => {
                metric! {
                    statsd_count!("ingester.account_update_error", 1, "owner" => &str_program_id, "error" => "de");
                }
                ret_id = Some(id);
            }
            Err(IngesterError::ParsingError(_)) => {
                metric! {
                    statsd_count!("ingester.account_update_error", 1, "owner" => &str_program_id, "error" => "parse");
                }
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
