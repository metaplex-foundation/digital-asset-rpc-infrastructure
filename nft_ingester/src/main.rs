mod backfiller;
pub mod config;
mod database;
pub mod error;
pub mod metrics;
mod program_transformers;
mod start;
mod stream;
pub mod tasks;
mod account_updates;
use start::start;
use tracing::log::error;

#[tokio::main]
async fn main() {
    let tasks = start().await;
    match tasks {
        Ok(mut tasks) => {
            // Wait for signal to shutdown
            match tokio::signal::ctrl_c().await {
                Ok(()) => {}
                Err(err) => {
                    error!("Unable to listen for shutdown signal: {}", err);
                }
            }
            tasks.shutdown().await;
        }
        Err(err) => {
            error!("Unable to start: {}", err);
        }
    }
}

// async fn service_transaction_stream<T: Messenger>(
//     pool: Pool<Postgres>,
//     tasks: UnboundedSender<TaskData>,
//     messenger_config: MessengerConfig,
// ) -> tokio::task::JoinHandle<()> {
//     tokio::spawn(async move {
//         // If we get crash, we want to retry.
//         loop {
//             let pool_cloned = pool.clone();
//             let tasks_cloned = tasks.clone();
//             let messenger_config_cloned = messenger_config.clone();

//             let result = tokio::spawn(async {
//                 let manager = Arc::new(ProgramTransformer::new(pool_cloned, tasks_cloned));
//                 let mut messenger = T::new(messenger_config_cloned).await.unwrap();
//                 println!("Setting up transaction listener");

//                 loop {
//                     if let Ok(data) = messenger
//                         .recv(TRANSACTION_STREAM, ConsumptionType::All)
//                         .await
//                     {
//                         let ids = handle_transaction(&manager, data).await;
//                         if !ids.is_empty() {
//                             if let Err(e) = messenger.ack_msg(TRANSACTION_STREAM, &ids).await {
//                                 println!("Error ACK-ing messages {:?}", e);
//                             }
//                         }
//                     }
//                 }
//             })
//             .await;

//             match result {
//                 Ok(_) => break,
//                 Err(err) if err.is_panic() => {
//                     statsd_count!("ingester.service_transaction_stream.task_panic", 1);
//                 }
//                 Err(err) => {
//                     let err = err.to_string();
//                     statsd_count!("ingester.service_transaction_stream.task_error", 1, "error" => &err);
//                 }
//             }
//         }
//     })
// }

// async fn handle_account(manager: &Arc<ProgramTransformer>, data: Vec<RecvData>) -> Vec<String> {
//     metric! {
//         statsd_gauge!("ingester.account_batch_size", data.len() as u64);
//     }

//     let tasks = FuturesUnordered::new();
//     for item in data.into_iter() {
//         tasks.push(async move {
//                 let id = item.id;
//             let mut ret_id = None;
//             if item.tries > 0 {
//                 metric! {
//                     statsd_count!("ingester.account_stream_redelivery", 1);
//                 }
//             }
//             let data = item.data;
//             // Get root of account info flatbuffers object.
//            if let Ok(account_update) = root_as_account_info(&data) {
//             let seen_at = Utc::now();
//             let str_program_id =
//                 bs58::encode(account_update.owner().unwrap().0.as_slice()).into_string();
//             metric! {
//                 statsd_count!("ingester.account_update_seen", 1, "owner" => &str_program_id);
//                 statsd_time!(
//                     "ingester.account_bus_ingest_time",
//                     (seen_at.timestamp_millis() - account_update.seen_at()) as u64,
//                     "owner" => &str_program_id
//                 );
//             }
//             let begin_processing = Instant::now();
//             let res = manager.handle_account_update(account_update).await;
//             match res {
//                 Ok(_) => {
//                     if item.tries == 0 {
//                         metric! {
//                             statsd_time!("ingester.account_proc_time", begin_processing.elapsed().as_millis() as u64, "owner" => &str_program_id);
//                         }
//                         metric! {
//                             statsd_count!("ingester.account_update_success", 1, "owner" => &str_program_id);
//                         }
//                     }
//                     ret_id = Some(id);
//                 }
//                 Err(err) if err == IngesterError::NotImplemented => {
//                     metric! {
//                         statsd_count!("ingester.account_not_implemented", 1, "owner" => &str_program_id, "error" => "ni");
//                     }
//                     ret_id = Some(id);
//                 }
//                 Err(IngesterError::DeserializationError(_)) => {
//                     metric! {
//                         statsd_count!("ingester.account_update_error", 1, "owner" => &str_program_id, "error" => "de");
//                     }
//                     ret_id = Some(id);
//                 }
//                 Err(IngesterError::ParsingError(_)) => {
//                     metric! {
//                         statsd_count!("ingester.account_update_error", 1, "owner" => &str_program_id, "error" => "parse");
//                     }
//                     ret_id = Some(id);
//                 }
//                 Err(err) => {
//                     println!("Error handling account update: {:?}", err);
//                     metric! {
//                         statsd_count!("ingester.account_update_error", 1, "owner" => &str_program_id, "error" => "u");
//                     }
//                 }
//             }
//         }
//             ret_id
//         });
//     }
//     tasks
//         .collect::<Vec<_>>()
//         .await
//         .into_iter()
//         .flatten()
//         .collect()
// }

// async fn handle_transaction(manager: &Arc<ProgramTransformer>, data: Vec<RecvData>) -> Vec<String> {
//     metric! {
//         statsd_gauge!("ingester.txn_batch_size", data.len() as u64);
//     }

//     let tasks = FuturesUnordered::new();
//     for item in data {
//         let manager = Arc::clone(manager);
//         tasks.push(async move {
//         let mut ret_id = None;
//         if item.tries > 0 {
//             metric! {
//                 statsd_count!("ingester.tx_stream_redelivery", 1);
//             }
//         }
//         let id = item.id.to_string();
//         let tx_data = item.data;
//         if let Ok(tx) = root_as_transaction_info(&tx_data) {
//             let signature = tx.signature().unwrap_or("NO SIG");
//             if let Some(si) = tx.slot_index() {
//                 let slt_idx = format!("{}-{}", tx.slot(), si);
//                 metric! {
//                     statsd_count!("ingester.transaction_event_seen", 1, "slot-idx" => &slt_idx);
//                 }
//             }
//             let seen_at = Utc::now();
//             metric! {
//                 statsd_time!(
//                     "ingester.bus_ingest_time",
//                     (seen_at.timestamp_millis() - tx.seen_at()) as u64
//                 );
//             }
//             let begin = Instant::now();
//             let res = manager.handle_transaction(&tx).await;
//             match res {
//                 Ok(_) => {
//                     if item.tries == 0 {
//                         metric! {
//                             statsd_time!("ingester.tx_proc_time", begin.elapsed().as_millis() as u64);
//                             statsd_count!("ingester.tx_ingest_success", 1);
//                         }
//                     } else {
//                         metric! {
//                             statsd_count!("ingester.tx_ingest_redeliver_success", 1);
//                         }
//                     }
//                     ret_id = Some(id);
//                 }
//                 Err(err) if err == IngesterError::NotImplemented => {
//                     metric! {
//                         statsd_count!("ingester.tx_not_implemented", 1);
//                     }
//                     ret_id = Some(id);
//                 }
//                 Err(err) => {
//                     println!("ERROR:txn: {:?} {:?}", signature, err);
//                     metric! {
//                         statsd_count!("ingester.tx_ingest_error", 1);
//                     }
//                 }
//             }
//         }
//         ret_id
//     });
//     }
//     tasks
//         .collect::<Vec<_>>()
//         .await
//         .into_iter()
//         .flatten()
//         .collect()
// }
