use crate::{
    account_updates::setup_account_stream_worker,
    backfiller::setup_backfiller,
    config::{setup_config, IngesterRole},
    database::setup_database,
    error::IngesterError,
    metric,
    metrics::setup_metrics,
    stream::{MessengerStreamManager, StreamSizeTimer},
    tasks::{BgTask, DownloadMetadataTask, TaskManager},
};

use crate::config::rand_string;
use cadence_macros::{is_global_default_set, statsd_count};
use chrono::Duration;
use plerkle_messenger::{redis_messenger::RedisMessenger, ACCOUNT_STREAM, TRANSACTION_STREAM};
use tokio::task::{JoinError, JoinSet};
use tracing::log::info;

pub async fn start() -> Result<JoinSet<Result<(), JoinError>>, IngesterError> {
    info!("Starting DASgester");
    // Setup Configuration and Metrics ---------------------------------------------
    // Pull Env variables into config struct
    let config = setup_config();
    // Optionally setup metrics if config demands it
    setup_metrics(&config);
    // One pool many clones, this thing is thread safe and send sync
    let database_pool = setup_database(config.clone()).await;
    // The role determines the processes that get run.
    let role = config.clone().role.unwrap_or(IngesterRole::All);

    // Tasks Setup -----------------------------------------------
    // This joinset maages all the tasks that are spawned.
    let mut tasks = JoinSet::new();
    let stream_metrics_timer = Duration::seconds(60).to_std().unwrap();

    // Stream Size Timers ----------------------------------------
    // Setup Stream Size Timers, these are small processes that run every 60 seconds and farm metrics for the size of the streams.
    // If metrics are disabled, these will not run.
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

    // BACKGROUND TASKS --------------------------------------------
    //Setup definitions for background tasks
    let bg_task_definitions: Vec<Box<dyn BgTask>> = vec![Box::new(DownloadMetadataTask {})];
    let mut background_task_manager =
        TaskManager::new(rand_string(), database_pool.clone(), bg_task_definitions);
    let bg_task_listener = background_task_manager
        .start_listener(role == IngesterRole::BackgroundTaskRunner || role == IngesterRole::All);
    // Always listen for background tasks unless we are the bg task runner
    if role != IngesterRole::BackgroundTaskRunner {
        tasks.spawn(bg_task_listener);
    }

    if role == IngesterRole::BackgroundTaskRunner || role == IngesterRole::All {
        tasks.spawn(background_task_manager.start_runner());
        if let Some(t) = timer_acc.start::<RedisMessenger>().await {
            tasks.spawn(t);
        }
        if let Some(t) = timer_txn.start::<RedisMessenger>().await {
            tasks.spawn(t);
        }
    }
    // Backfiller Setup ------------------------------------------
    if role == IngesterRole::Backfiller || role == IngesterRole::All {
        let backfiller =
            setup_backfiller::<RedisMessenger>(database_pool.clone(), config.clone()).await;
        tasks.spawn(backfiller);
    }
    // Stream Consumers Setup -------------------------------------
    if role == IngesterRole::Ingester || role == IngesterRole::All {
        // This is how we send new bg tasks
        let bg_task_sender = background_task_manager.get_sender().unwrap();
        let mut ams = MessengerStreamManager::new(ACCOUNT_STREAM, config.messenger_config.clone());
        let max_account_workers = config.account_stream_worker_count.unwrap_or(3);
        for i in 0..max_account_workers {
            let stream = if i == max_account_workers - 1 {
                ams.listen::<RedisMessenger>(plerkle_messenger::ConsumptionType::Redeliver)
            } else {
                ams.listen::<RedisMessenger>(plerkle_messenger::ConsumptionType::New)
            }?;
            tasks.spawn(setup_account_stream_worker::<RedisMessenger>(
                database_pool.clone(),
                bg_task_sender.clone(),
                stream,
            ));
        }
    }

    let roles_str = role.to_string();
    metric! {
     statsd_count!("ingester.startup", 1, "role" => &roles_str);
    }

    Ok(tasks)
}
