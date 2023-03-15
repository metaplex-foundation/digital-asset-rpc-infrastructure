use crate::{
    account_updates::account_worker,
    ack::ack_worker,
    backfiller::setup_backfiller,
    config::{setup_config, IngesterRole},
    database::setup_database,
    error::IngesterError,
    metric,
    metrics::setup_metrics,
    stream::StreamSizeTimer,
    tasks::{BgTask, DownloadMetadataTask, TaskManager},
    transaction_notifications::transaction_worker,
};

use crate::config::rand_string;
use cadence_macros::{is_global_default_set, statsd_count};
use chrono::Duration;
use log::{error, info};
use plerkle_messenger::{redis_messenger::RedisMessenger, ACCOUNT_STREAM, TRANSACTION_STREAM};
use tokio::task::{JoinError, JoinSet};

pub async fn start() -> Result<(), IngesterError> {
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
    let stream_metrics_timer = Duration::seconds(60).to_std().unwrap();

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
    // Stream Consumers Setup -------------------------------------
    if role == IngesterRole::Ingester || role == IngesterRole::All {
        // This is how we send new bg tasks
        let bg_task_sender = background_task_manager.get_sender().unwrap();
        let (ack_task, ack_sender) =
            ack_worker::<RedisMessenger>(ACCOUNT_STREAM, config.messenger_config.clone()).await;
        tasks.spawn(ack_task);

        let max_account_workers = config.get_account_stream_worker_count();
       
            let stream = account_worker::<RedisMessenger>(
                database_pool.clone(),
                ACCOUNT_STREAM,
                config.messenger_config.clone(),
                bg_task_sender.clone(),
                ack_sender.clone(),
            )
            .await?;
            tasks.spawn(stream);
        

       
            let stream = transaction_worker::<RedisMessenger>(
                database_pool.clone(),
                ACCOUNT_STREAM,
                config.messenger_config.clone(),
                bg_task_sender.clone(),
                ack_sender.clone(),
            )
            .await?;
            tasks.spawn(stream);
        
    }
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

    let roles_str = role.to_string();
    metric! {
        statsd_count!("ingester.startup", 1, "role" => &roles_str);
    }

    while let Some(t) = tasks.join_next().await {
        match t {
            Ok(_) => {
                error!("Task completed");
            }
            Err(e) => {
                error!("Task panicked: {}", e);
            }
        }
    }
    Ok(())
}
