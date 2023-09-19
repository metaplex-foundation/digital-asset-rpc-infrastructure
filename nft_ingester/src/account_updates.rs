use std::sync::Arc;

use crate::{
    metric, metrics::capture_result, program_transformers::ProgramTransformer, tasks::TaskData,
};
use cadence_macros::{is_global_default_set, statsd_count, statsd_time};
use chrono::Utc;
use log::{debug, error};
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
    config: MessengerConfig,
    bg_task_sender: UnboundedSender<TaskData>,
    ack_channel: UnboundedSender<(&'static str, String)>,
    consumption_type: ConsumptionType,
    stream_key: &'static str,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let source = T::new(config).await;
        if let Ok(mut msg) = source {
            let manager = Arc::new(ProgramTransformer::new(pool, bg_task_sender));
            loop {
                let e = msg.recv(stream_key, consumption_type.clone()).await;
                let mut tasks = JoinSet::new();
                match e {
                    Ok(data) => {
                        let len = data.len();
                        for item in data {
                            tasks.spawn(handle_account(Arc::clone(&manager), item, stream_key));
                        }
                        if len > 0 {
                            debug!("Processed {} accounts", len);
                        }
                    }
                    Err(e) => {
                        error!("Error receiving from account stream: {}", e);
                        metric! {
                            statsd_count!("ingester.stream.receive_error", 1, "stream" => stream_key);
                        }
                    }
                }
                while let Some(res) = tasks.join_next().await {
                    if let Ok(id) = res {
                        if let Some(id) = id {
                            let send = ack_channel.send((stream_key, id));
                            if let Err(err) = send {
                                metric! {
                                    error!("Account stream ack error: {}", err);
                                    statsd_count!("ingester.stream.ack_error", 1, "stream" => stream_key);
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

async fn handle_account(manager: Arc<ProgramTransformer>, item: RecvData, stream_key: &'static str) -> Option<String> {
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
            statsd_count!("ingester.seen", 1, "owner" => &str_program_id, "stream" => stream_key);
            let seen_at = Utc::now();
            statsd_time!(
                "ingester.bus_ingest_time",
                (seen_at.timestamp_millis() - account_update.seen_at()) as u64,
                "owner" => &str_program_id,
                "stream" => stream_key
            );
        }
        let mut account = None;
        if let Some(pubkey) = account_update.pubkey() {
            account = Some(bs58::encode(pubkey.0.as_slice()).into_string());
        }
        let begin_processing = Instant::now();
        let res = manager.handle_account_update(account_update).await;
        let should_ack = capture_result(
            id.clone(),
            stream_key,
            ("owner", &str_program_id),
            item.tries,
            res,
            begin_processing,
            None,
            account,
        );
        if should_ack {
            ret_id = Some(id);
        }
    }
    ret_id
}
