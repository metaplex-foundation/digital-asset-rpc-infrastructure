mod backfiller;
mod error;
mod metrics;
mod program_transformers;
mod tasks;

use crate::{
    backfiller::backfiller,
    error::IngesterError,
    metrics::safe_metric,
    program_transformers::ProgramTransformer,
    tasks::{BgTask, TaskManager},
};
use blockbuster::instruction::{order_instructions, InstructionBundle};
use cadence::{BufferedUdpMetricSink, QueuingMetricSink, StatsdClient};
use cadence_macros::{set_global_default, statsd_count, statsd_time};
use chrono::Utc;
use figment::{providers::Env, Figment};
use plerkle_messenger::{
    Messenger, MessengerConfig, RedisMessenger, ACCOUNT_STREAM, TRANSACTION_STREAM,
};
use plerkle_serialization::{root_as_account_info, root_as_transaction_info};
use serde::Deserialize;
use sqlx::{self, postgres::PgPoolOptions, Pool, Postgres};
use std::{collections::HashSet, net::UdpSocket};
use tokio::sync::mpsc::UnboundedSender;

// Types and constants used for Figment configuration items.
pub type DatabaseConfig = figment::value::Dict;

pub const DATABASE_URL_KEY: &str = "url";
pub const DATABASE_LISTENER_CHANNEL_KEY: &str = "listener_channel";

pub type RpcConfig = figment::value::Dict;

pub const RPC_URL_KEY: &str = "url";
pub const RPC_COMMITMENT_KEY: &str = "commitment";

// Struct used for Figment configuration items.
#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct IngesterConfig {
    pub database_config: DatabaseConfig,
    pub messenger_config: MessengerConfig,
    pub rpc_config: RpcConfig,
    pub metrics_port: Option<u16>,
    pub metrics_host: Option<String>,
}

fn setup_metrics(config: &IngesterConfig) {
    let uri = config.metrics_host.clone();
    let port = config.metrics_port.clone();
    if uri.is_some() || port.is_some() {
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let host = (uri.unwrap(), port.unwrap());
        let udp_sink = BufferedUdpMetricSink::from(host, socket).unwrap();
        let queuing_sink = QueuingMetricSink::from(udp_sink);
        let client = StatsdClient::from_sink("das_ingester", queuing_sink);
        set_global_default(client);
    }
}

#[tokio::main]
async fn main() {
    // Read config.
    println!("Starting DASgester");
    let config: IngesterConfig = Figment::new()
        .join(Env::prefixed("INGESTER_"))
        .extract()
        .map_err(|config_error| IngesterError::ConfigurationError {
            msg: format!("{}", config_error),
        })
        .unwrap();
    // Get database config.
    let url = config
        .database_config
        .get(&*DATABASE_URL_KEY)
        .and_then(|u| u.clone().into_string())
        .ok_or(IngesterError::ConfigurationError {
            msg: format!("Database connection string missing: {}", DATABASE_URL_KEY),
        })
        .unwrap();
    // Setup Postgres.
    let mut tasks = vec![];
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    let background_task_manager =
        TaskManager::new("background-tasks".to_string(), pool.clone()).unwrap();
    // Service streams as separate concurrent processes.
    println!("Setting up tasks");
    setup_metrics(&config);
    tasks.push(
        service_transaction_stream::<RedisMessenger>(
            pool.clone(),
            background_task_manager.get_sender(),
            config.messenger_config.clone(),
        )
        .await,
    );
    tasks.push(
        service_account_stream::<RedisMessenger>(
            pool.clone(),
            background_task_manager.get_sender(),
            config.messenger_config.clone(),
        )
        .await,
    );
    safe_metric(|| {
        statsd_count!("ingester.startup", 1);
    });

    tasks.push(backfiller::<RedisMessenger>(pool.clone(), config.clone()).await);
    // Wait for ctrl-c.
    match tokio::signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            println!("Unable to listen for shutdown signal: {}", err);
            // We also shut down in case of error.
        }
    }

    // Kill all tasks.
    for task in tasks {
        task.abort();
    }
}

async fn service_transaction_stream<T: Messenger>(
    pool: Pool<Postgres>,
    tasks: UnboundedSender<Box<dyn BgTask>>,
    messenger_config: MessengerConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let manager = ProgramTransformer::new(pool, tasks);
        let mut messenger = T::new(messenger_config).await.unwrap();
        println!("Setting up transaction listener");
        loop {
            // This call to messenger.recv() blocks with no timeout until
            // a message is received on the stream.
            if let Ok(data) = messenger.recv(TRANSACTION_STREAM).await {
                //TODO -> do not ACK until this is Ok(())
                handle_transaction(&manager, data).await
            }
        }
    })
}

async fn service_account_stream<T: Messenger>(
    pool: Pool<Postgres>,
    tasks: UnboundedSender<Box<dyn BgTask>>,
    messenger_config: MessengerConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let manager = ProgramTransformer::new(pool, tasks);
        let mut messenger = T::new(messenger_config).await.unwrap();
        println!("Setting up account listener");
        loop {
            // This call to messenger.recv() blocks with no timeout until
            // a message is received on the stream.
            if let Ok(data) = messenger.recv(ACCOUNT_STREAM).await {
                println!("Received account data");
                handle_account(&manager, data).await
            }
        }
    })
}

async fn handle_account(manager: &ProgramTransformer, data: Vec<(i64, &[u8])>) {
    for (_message_id, data) in data {
        // Get root of account info flatbuffers object.
        let account_update = match root_as_account_info(data) {
            Err(err) => {
                println!("Flatbuffers AccountInfo deserialization error: {err}");
                continue;
            }
            Ok(account_update) => account_update,
        };
        safe_metric(|| {
            statsd_count!("ingester.account_update_seen", 1);
        });

        println!(
            "Received account data {:?}",
            account_update
                .owner()
                .map(|e| bs58::encode(e.0.as_slice()).into_string())
        );
        let begin_processing = Utc::now();
        let res = manager.handle_account_update(&account_update).await;
        let finish_processing = Utc::now();
        match res {
            Ok(_) => {
                safe_metric(|| {
                    let proc_time = (finish_processing.timestamp_millis()
                        - begin_processing.timestamp_millis())
                        as u64;
                    statsd_time!("ingester.account_proc_time", proc_time);
                });
                safe_metric(|| {
                    statsd_count!("ingester.account_update_success", 1);
                });
            }
            Err(err) => {
                println!("Error handling account update: {:?}", err);
                safe_metric(|| {
                    statsd_count!("ingester.account_update_error", 1);
                });
            }
        }
    }
}

async fn handle_transaction(manager: &ProgramTransformer, data: Vec<(i64, &[u8])>) {
    for (_message_id, tx_data) in data.iter() {
        //TODO -> Dedupe the stream, the stream could have duplicates as a way of ensuring fault tolerance if one validator node goes down.
        //  Possible solution is dedup on the plerkle side but this doesnt follow our principle of getting messages out of the validator asd fast as possible.
        //  Consider a Messenger Implementation detail the deduping of whats in this stream so that
        //  1. only 1 ingest instance picks it up, two the stream coming out of the ingester can be considered deduped
        //
        // can we paralellize this : yes

        // Get root of transaction info flatbuffers object.
        if let Ok(tx) = root_as_transaction_info(tx_data) {
            //TODO -> load this from config in the transformer
            let bgum = blockbuster::programs::bubblegum::program_id();
            let mut programs: HashSet<&[u8]> = HashSet::new();
            programs.insert(bgum.as_ref());

            let instructions = order_instructions(programs, &tx);
            let keys = tx.account_keys();
            if let Some(si) = tx.slot_index() {
                let slt_idx = format!("{}-{}", tx.slot(), si);
                safe_metric(|| {
                    statsd_count!("ingester.transaction_event_seen", 1, "slot-idx" => &slt_idx);
                });
            }
            let seen_at = Utc::now();
            for (outer_ix, inner_ix) in instructions {
                let (program, instruction) = outer_ix;
                let bundle = InstructionBundle {
                    txn_id: "",
                    program,
                    instruction,
                    inner_ix,
                    keys: keys.unwrap(),
                    slot: tx.slot(),
                };
                let (program, _) = &outer_ix;
                safe_metric(|| {
                    statsd_time!(
                        "ingester.bus_ingest_time",
                        (seen_at.timestamp_millis() - tx.seen_at()) as u64
                    );
                });
                manager
                    .handle_instruction(&bundle)
                    .await
                    .expect("Processing Failed");
                let finished_at = Utc::now();
                let str_program_id = bs58::encode(program.0.as_slice()).into_string();
                safe_metric(|| {
                    statsd_time!("ingester.ix_process_time", (finished_at.timestamp_millis() - tx.seen_at()) as u64, "program_id" => &str_program_id);
                });
            }
            // TODO -> DLQ message if it failed.
        }
    }
}
// Associates logs with the given program ID
