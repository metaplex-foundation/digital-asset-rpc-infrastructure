use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    config::rand_string, error::IngesterError, metric, metrics::capture_result,
    program_transformers::ProgramTransformer, tasks::TaskData,
};
use cadence_macros::{is_global_default_set, statsd_count, statsd_gauge, statsd_time};
use chrono::Utc;
use futures::{stream::FuturesUnordered, StreamExt};
use log::{debug, error, info};
use plerkle_messenger::{ConsumptionType, Messenger, MessengerConfig, RecvData, ACCOUNT_STREAM};
use plerkle_serialization::root_as_account_info;
use sqlx::{Pool, Postgres};
use tokio::{
    sync::mpsc::UnboundedSender,
    task::{JoinHandle, JoinSet},
    time::Instant,
};

pub fn account_worker<T: Messenger>(
    pool: Pool<Postgres>,
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
                let e = msg.recv(ACCOUNT_STREAM, consumption_type.clone()).await;
                match e {
                    Ok(data) => {
                        let mut futures = FuturesUnordered::new();
                        for item in data {
                            let m = Arc::clone(&manager);
                            let s = ack_channel.clone();
                            futures.push(async move {
                                if let Some(id) = handle_account(m, item).await {
                                    let send = s.send(id);
                                    if let Err(err) = send {
                                        metric! {
                                            error!("Account stream ack error: {}", err);
                                            statsd_count!("ingester.stream.ack_error", 1, "stream" => ACCOUNT_STREAM);
                                        }
                                    }
                                }
                            });
                        }
                        while let Some(_) = futures.next().await {
                            info!("Processed {} account updates", futures.len());
                        }
                    }
                    Err(e) => {
                        error!("Error receiving from account stream: {}", e);
                        metric! {
                            statsd_count!("ingester.stream.receive_error", 1, "stream" => ACCOUNT_STREAM);
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
    let data = item.data;
    if item.tries > 0 {
        metric! {
            statsd_count!("ingester.account_stream_redelivery", 1);
        }
    }
    // Get root of account info flatbuffers object.
    if let Ok(account_update) = root_as_account_info(&data) {
        let str_program_id =
            bs58::encode(account_update.owner().unwrap().0.as_slice()).into_string();
        metric! {
            statsd_count!("ingester.seen", 1, "owner" => &str_program_id, "stream" => ACCOUNT_STREAM);
            let seen_at = Utc::now();
            statsd_time!(
                "ingester.bus_ingest_time",
                (seen_at.timestamp_millis() - account_update.seen_at()) as u64,
                "owner" => &str_program_id,
                "stream" => ACCOUNT_STREAM
            );
        }
        let begin_processing = Instant::now();
        let res = manager.handle_account_update(account_update).await;
        ret_id = capture_result(
            id,
            ACCOUNT_STREAM,
            ("owner", &str_program_id),
            item.tries,
            res,
            begin_processing,
        );
    }
    ret_id
}
