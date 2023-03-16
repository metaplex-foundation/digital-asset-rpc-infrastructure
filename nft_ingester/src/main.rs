mod account_updates;
mod ack;
mod backfiller;
pub mod config;
mod database;
pub mod error;
pub mod metrics;
mod program_transformers;
mod stream;
pub mod tasks;
mod transaction_notifications;

use env_logger;

use crate::{
    account_updates::account_worker,
    ack::ack_worker,
    backfiller::setup_backfiller,
    config::{setup_config, IngesterRole},
    database::setup_database,
    error::IngesterError,
    metrics::setup_metrics,
    stream::StreamSizeTimer,
    tasks::{BgTask, DownloadMetadataTask, TaskManager},
    transaction_notifications::transaction_worker,
};

use crate::config::rand_string;
use cadence_macros::{is_global_default_set, statsd_count};
use chrono::Duration;
use log::{error, info};
use plerkle_messenger::{
    redis_messenger::RedisMessenger, ConsumptionType, ACCOUNT_STREAM, TRANSACTION_STREAM,
};
use tokio::{
    signal,
    task::{JoinError, JoinSet},
};

#[tokio::main(flavor = "multi_thread")]
pub async fn main() -> Result<(), IngesterError> {
    env_logger::init();
    info!("Starting nft_ingester");
    // Setup Configuration and Metrics ---------------------------------------------
    // Pull Env variables into config struct
    let config = setup_config();
    // Optionally setup metrics if config demands it
    setup_metrics(&config);
    // One pool many clones, this thing is thread safe and send sync
    let database_pool = setup_database(config.clone()).await;
    // The role determines the processes that get run.
    let role = config.clone().role.unwrap_or(IngesterRole::All);
    info!("Starting Program with Role {}", role);
    // Tasks Setup -----------------------------------------------
    // This joinset maages all the tasks that are spawned.
    let mut tasks = JoinSet::new();
    let stream_metrics_timer = Duration::seconds(30).to_std().unwrap();

    // BACKGROUND TASKS --------------------------------------------
    //Setup definitions for background tasks
    let bg_task_definitions: Vec<Box<dyn BgTask>> = vec![Box::new(DownloadMetadataTask {})];

    let mut background_task_manager =
        TaskManager::new(rand_string(), database_pool.clone(), bg_task_definitions);
    // This is how we send new bg tasks
    let bg_task_listener = background_task_manager
        .start_listener(role == IngesterRole::BackgroundTaskRunner || role == IngesterRole::All);
    let bg_task_sender = background_task_manager.get_sender().unwrap();
    // Always listen for background tasks unless we are the bg task runner
    if role != IngesterRole::BackgroundTaskRunner {
        tasks.spawn(bg_task_listener);
    }
    let mut timer_acc = StreamSizeTimer::new(
        stream_metrics_timer,
        config.messenger_config.clone(),
        ACCOUNT_STREAM,
    )?;
    let mut timer_txn = StreamSizeTimer::new(
        stream_metrics_timer.clone(),
        config.messenger_config.clone(),
        TRANSACTION_STREAM,
    )?;

    if let Some(t) = timer_acc.start::<RedisMessenger>().await {
        tasks.spawn(t);
    }
    if let Some(t) = timer_txn.start::<RedisMessenger>().await {
        tasks.spawn(t);
    }

    // Stream Consumers Setup -------------------------------------
    if role == IngesterRole::Ingester || role == IngesterRole::All {
        let (ack_task, ack_sender) =
            ack_worker::<RedisMessenger>(ACCOUNT_STREAM, config.messenger_config.clone());
        tasks.spawn(ack_task);
        let account = account_worker::<RedisMessenger>(
            database_pool.clone(),
            ACCOUNT_STREAM,
            config.messenger_config.clone(),
            bg_task_sender.clone(),
            ack_sender.clone(),
            ConsumptionType::New,
        );
        tasks.spawn(tokio::task::unconstrained(account));
        let account_red = account_worker::<RedisMessenger>(
            database_pool.clone(),
            ACCOUNT_STREAM,
            config.messenger_config.clone(),
            bg_task_sender.clone(),
            ack_sender.clone(),
            ConsumptionType::Redeliver,
        );
        tasks.spawn(account_red);

        let txns = transaction_worker::<RedisMessenger>(
            database_pool.clone(),
            TRANSACTION_STREAM,
            config.messenger_config.clone(),
            bg_task_sender.clone(),
            ack_sender.clone(),
            ConsumptionType::All,
        );
        tasks.spawn(tokio::task::unconstrained(txns));
        let txns_red = transaction_worker::<RedisMessenger>(
            database_pool.clone(),
            TRANSACTION_STREAM,
            config.messenger_config.clone(),
            bg_task_sender.clone(),
            ack_sender.clone(),
            ConsumptionType::Redeliver,
        );
        tasks.spawn(txns_red);
    }
    // Stream Size Timers ----------------------------------------
    // Setup Stream Size Timers, these are small processes that run every 60 seconds and farm metrics for the size of the streams.
    // If metrics are disabled, these will not run.
    if role == IngesterRole::BackgroundTaskRunner || role == IngesterRole::All {
        tasks.spawn(background_task_manager.start_runner());
    }
    // Backfiller Setup ------------------------------------------
    if role == IngesterRole::Backfiller || role == IngesterRole::All {
        let backfiller = setup_backfiller::<RedisMessenger>(database_pool.clone(), config.clone());
        tasks.spawn(backfiller);
    }

    let roles_str = role.to_string();
    metric! {
        statsd_count!("ingester.startup", 1, "role" => &roles_str);
    }

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }

    tasks.shutdown().await;

    Ok(())
}
