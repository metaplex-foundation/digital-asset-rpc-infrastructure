use {
    crate::{
        metric,
        metrics::capture_result,
        plerkle::{into_program_transformer_err, PlerkleTransactionInfo},
        tasks::{create_download_metadata_notifier, TaskData},
    },
    cadence_macros::{is_global_default_set, statsd_count, statsd_time},
    chrono::Utc,
    log::{debug, error},
    plerkle_messenger::{ConsumptionType, Messenger, MessengerConfig, RecvData},
    plerkle_serialization::root_as_transaction_info,
    program_transformers::ProgramTransformer,
    sqlx::{Pool, Postgres},
    std::sync::Arc,
    tokio::{
        sync::mpsc::UnboundedSender,
        task::{JoinHandle, JoinSet},
        time::Instant,
    },
};

pub fn transaction_worker<T: Messenger>(
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
            let manager = Arc::new(ProgramTransformer::new(
                pool,
                create_download_metadata_notifier(bg_task_sender),
            ));
            loop {
                let e = msg.recv(stream_key, consumption_type.clone()).await;
                let mut tasks = JoinSet::new();
                match e {
                    Ok(data) => {
                        let len = data.len();
                        for item in data {
                            tasks.spawn(handle_transaction(Arc::clone(&manager), item, stream_key));
                        }
                        if len > 0 {
                            debug!("Processed {} txns", len);
                        }
                    }
                    Err(e) => {
                        error!("Error receiving from txn stream: {}", e);
                        metric! {
                            statsd_count!("ingester.stream.receive_error", 1, "stream" => stream_key);
                        }
                    }
                }
                while let Some(res) = tasks.join_next().await {
                    if let Ok(Some(id)) = res {
                        let send = ack_channel.send((stream_key, id));
                        if let Err(err) = send {
                            metric! {
                                    error!("Txn stream ack error: {}", err);
                                    statsd_count!("ingester.stream.ack_error", 1, "stream" => stream_key);
                            }
                        }
                    }
                }
            }
        }
    })
}

async fn handle_transaction(
    manager: Arc<ProgramTransformer>,
    item: RecvData,
    stream_key: &'static str,
) -> Option<String> {
    let mut ret_id = None;
    if item.tries > 0 {
        metric! {
            statsd_count!("ingester.stream_redelivery", 1, "stream" => stream_key);
        }
    }
    let id = item.id.to_string();
    let tx_data = item.data;
    if let Ok(tx) = root_as_transaction_info(&tx_data) {
        let signature = tx.signature().unwrap_or("NO SIG");
        debug!("Received transaction: {}", signature);
        metric! {
            statsd_count!("ingester.seen", 1, "stream" => stream_key);
        }
        let seen_at = Utc::now();
        metric! {
            statsd_time!(
                "ingester.bus_ingest_time",
                (seen_at.timestamp_millis() - tx.seen_at()) as u64,
                "stream" => stream_key
            );
        }

        let begin = Instant::now();
        let transaction_info = PlerkleTransactionInfo(tx)
            .try_into()
            .map_err(into_program_transformer_err)
            .ok()?;
        let res = manager.handle_transaction(&transaction_info).await;
        let should_ack = capture_result(
            id.clone(),
            stream_key,
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
