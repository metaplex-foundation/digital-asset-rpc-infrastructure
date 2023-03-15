use std::sync::Arc;

use crate::{
    error::IngesterError, metric, program_transformers::ProgramTransformer,
    tasks::TaskData,
};
use cadence_macros::{is_global_default_set, statsd_count, statsd_time};
use chrono::Utc;
use log::{error, info};
use plerkle_messenger::{Messenger, RecvData, MessengerConfig, ConsumptionType};
use plerkle_serialization::root_as_transaction_info;
use sqlx::{Pool, Postgres};
use tokio::{sync::mpsc::UnboundedSender, task::{JoinHandle, JoinSet}, time::Instant};

pub async fn transaction_worker<T: Messenger>(pool: Pool<Postgres>, 
    stream: &'static str,
    config: MessengerConfig, 
    bg_task_sender: UnboundedSender<TaskData>,
    ack_channel: UnboundedSender<String>
) -> Result<JoinHandle<()>, IngesterError> {
    let t = tokio::spawn(async move {
        let source = T::new(config).await;
        if let Ok(mut msg) = source{
            let manager = Arc::new(ProgramTransformer::new(pool, bg_task_sender));
            loop {
                info!("{}: ", stream);
                if let Ok(data) = msg.recv(&stream, ConsumptionType::All).await {
                    let mut tasks = JoinSet::new();
                    for item in data {
                        tasks.spawn(handle_transaction(Arc::clone(&manager), item));
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
            }
        }
    });
    Ok(t)
}



async fn handle_transaction(manager: Arc<ProgramTransformer>, item: RecvData) -> Option<String> {
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
