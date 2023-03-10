use std::sync::Arc;

use crate::{
    error::IngesterError, metric, program_transformers::ProgramTransformer,
    stream::MessengerDataStream, tasks::TaskData,
};
use cadence_macros::{is_global_default_set, statsd_count, statsd_time};
use chrono::Utc;
use futures::StreamExt;
use plerkle_messenger::{Messenger, RecvData};
use plerkle_serialization::root_as_transaction_info;
use sqlx::{Pool, Postgres};
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle, time::Instant};

pub fn setup_transaction_stream_worker<T: Messenger>(
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
            if let Some(id) = handle_transaction(&manager, item).await {
                acks.push(id);
                if last_ack.elapsed().as_secs() > 1 {
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
async fn handle_transaction(manager: &Arc<ProgramTransformer>, item: RecvData) -> Option<String> {
    let mut ret_id = None;
    if item.tries > 0 {
        metric! {
            statsd_count!("ingester.tx_stream_redelivery", 1);
        }
    }
    let id = item.id.to_string();
    let tx_data = item.data;
    if let Ok(tx) = root_as_transaction_info(&tx_data) {
        let signature = tx.signature().unwrap_or("NO SIG");
        if let Some(si) = tx.slot_index() {
            let slt_idx = format!("{}-{}", tx.slot(), si);
            metric! {
                statsd_count!("ingester.transaction_event_seen", 1, "slot-idx" => &slt_idx);
            }
        }
        let seen_at = Utc::now();
        metric! {
            statsd_time!(
                "ingester.bus_ingest_time",
                (seen_at.timestamp_millis() - tx.seen_at()) as u64
            );
        }
        let begin = Instant::now();
        let res = manager.handle_transaction(&tx).await;
        match res {
            Ok(_) => {
                if item.tries == 0 {
                    metric! {
                        statsd_time!("ingester.tx_proc_time", begin.elapsed().as_millis() as u64);
                        statsd_count!("ingester.tx_ingest_success", 1);
                    }
                } else {
                    metric! {
                        statsd_count!("ingester.tx_ingest_redeliver_success", 1);
                    }
                }
                ret_id = Some(id);
            }
            Err(err) if err == IngesterError::NotImplemented => {
                metric! {
                    statsd_count!("ingester.tx_not_implemented", 1);
                }
                ret_id = Some(id);
            }
            Err(err) => {
                println!("ERROR:txn: {:?} {:?}", signature, err);
                metric! {
                    statsd_count!("ingester.tx_ingest_error", 1);
                }
            }
        }
    }
    ret_id
}
