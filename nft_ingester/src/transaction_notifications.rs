use std::sync::Arc;

use crate::{
    metric, metrics::capture_result, program_transformers::ProgramTransformer, tasks::TaskData,
};
use cadence_macros::{is_global_default_set, statsd_count, statsd_time};
use chrono::Utc;
use log::{debug, error};
use plerkle_messenger::{
    ConsumptionType, Messenger, MessengerConfig, RecvData, TRANSACTION_STREAM,
};
use plerkle_serialization::root_as_transaction_info;

use sqlx::{Pool, Postgres};
use tokio::{
    sync::mpsc::UnboundedSender,
    task::{JoinHandle, JoinSet},
    time::Instant,
};

pub fn transaction_worker<T: Messenger>(
    pool: Pool<Postgres>,
    config: MessengerConfig,
    bg_task_sender: UnboundedSender<TaskData>,
    ack_channel: UnboundedSender<(&'static str, String)>,
    consumption_type: ConsumptionType,
    cl_audits: bool,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let source = T::new(config).await;
        if let Ok(mut msg) = source {
            let manager = Arc::new(ProgramTransformer::new(pool, bg_task_sender, cl_audits));
            loop {
                let e = msg.recv(TRANSACTION_STREAM, consumption_type.clone()).await;
                let mut tasks = JoinSet::new();
                match e {
                    Ok(data) => {
                        let len = data.len();
                        for item in data {
                            tasks.spawn(handle_transaction(Arc::clone(&manager), item));
                        }
                        if len > 0 {
                            debug!("Processed {} txns", len);
                        }
                    }
                    Err(e) => {
                        error!("Error receiving from txn stream: {}", e);
                        metric! {
                            statsd_count!("ingester.stream.receive_error", 1, "stream" => TRANSACTION_STREAM);
                        }
                    }
                }
                while let Some(res) = tasks.join_next().await {
                    if let Ok(id) = res {
                        if let Some(id) = id {
                            let send = ack_channel.send((TRANSACTION_STREAM, id));
                            if let Err(err) = send {
                                metric! {
                                    error!("Txn stream ack error: {}", err);
                                    statsd_count!("ingester.stream.ack_error", 1, "stream" => TRANSACTION_STREAM);
                                }
                            }
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
            statsd_count!("ingester.stream_redelivery", 1, "stream" => TRANSACTION_STREAM);
        }
    }
    let id = item.id.to_string();
    let tx_data = item.data;
    if let Ok(tx) = root_as_transaction_info(&tx_data) {
        let signature = tx.signature().unwrap_or("NO SIG");
        debug!("Received transaction: {}", signature);
        metric! {
            statsd_count!("ingester.seen", 1, "stream" => TRANSACTION_STREAM);
        }
        let seen_at = Utc::now();
        metric! {
            statsd_time!(
                "ingester.bus_ingest_time",
                (seen_at.timestamp_millis() - tx.seen_at()) as u64,
                "stream" => TRANSACTION_STREAM
            );
        }

        let begin = Instant::now();
        let res = manager.handle_transaction(&tx).await;
        let should_ack = capture_result(
            id.clone(),
            TRANSACTION_STREAM,
            ("txn", "txn"),
            item.tries,
            res,
            begin,
            tx.signature(),
            None,
        );
        if should_ack {
            ret_id = Some(id);
        }
    }
    ret_id
}
