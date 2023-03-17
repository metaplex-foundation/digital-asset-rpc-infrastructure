use std::sync::Arc;

use crate::{
    config::rand_string, error::IngesterError, metric, metrics::capture_result,
    program_transformers::ProgramTransformer, tasks::TaskData,
};
use cadence_macros::{is_global_default_set, statsd_count, statsd_gauge, statsd_time};
use chrono::Utc;
use futures::{stream::FuturesUnordered, StreamExt};
use log::{debug, error, info};
use plerkle_messenger::{
    ConsumptionType, Messenger, MessengerConfig, RecvData, TRANSACTION_STREAM,
};
use plerkle_serialization::root_as_transaction_info;

use sqlx::{Pool, Postgres};
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle, time::Instant};

pub fn transaction_worker<T: Messenger>(
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
                        let mut futures = FuturesUnordered::new();
                        for item in data {
                            let m = Arc::clone(&manager);
                            let s = ack_channel.clone();
                            futures.push(async move {
                                if let Some(id) = handle_transaction(m, item).await {
                                    let send = s.send(id);
                                    if let Err(err) = send {
                                        metric! {
                                            error!("Account stream ack error: {}", err);
                                            statsd_count!("ingester.stream.ack_error", 1, "stream" => TRANSACTION_STREAM);
                                        }
                                    }
                                }
                            });
                        }
                        while let Some(_) = futures.next().await {
                            info!("Processed {} transactions", futures.len());
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
        debug!("Received transaction: {}", signature);
        metric! {
            statsd_count!("ingester.seen", 1);
        }
        let seen_at = Utc::now();
        statsd_time!(
            "ingester.bus_ingest_time",
            (seen_at.timestamp_millis() - tx.seen_at()) as u64
        );
        let begin = Instant::now();
        let res = manager.handle_transaction(&tx).await;
        ret_id = capture_result(
            id,
            TRANSACTION_STREAM,
            ("txn", "txn"),
            item.tries,
            res,
            begin,
        );
    }
    ret_id
}
