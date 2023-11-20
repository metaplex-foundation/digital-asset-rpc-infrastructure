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

use crate::{
    account_updates::account_worker,
    ack::ack_worker,
    backfiller::setup_backfiller,
    config::{init_logger, rand_string, setup_config, IngesterRole},
    database::setup_database,
    error::IngesterError,
    metrics::setup_metrics,
    stream::StreamSizeTimer,
    tasks::{BgTask, DownloadMetadataTask, TaskManager},
    transaction_notifications::transaction_worker,
};
use cadence_macros::{is_global_default_set, statsd_count};
use chrono::Duration;
use clap::{arg, command, value_parser};
use log::{error, info};
use plerkle_messenger::{
    redis_messenger::RedisMessenger, ConsumptionType, ACCOUNT_BACKFILL_STREAM, ACCOUNT_STREAM,
    TRANSACTION_BACKFILL_STREAM, TRANSACTION_STREAM,
};
use std::{path::PathBuf, time};
use tokio::{signal, task::JoinSet};

#[tokio::main(flavor = "multi_thread")]
pub async fn main() -> Result<(), IngesterError> {
    init_logger();
    info!("Starting nft_ingester");

    let matches = command!()
        .arg(
            arg!(
                -c --config <FILE> "Sets a custom config file"
            )
            // We don't have syntax yet for optional options, so manually calling `required`
            .required(false)
            .value_parser(value_parser!(PathBuf)),
        )
        .get_matches();

    let config_path = matches.get_one::<PathBuf>("config");
    if let Some(config_path) = config_path {
        println!("Loading config from: {}", config_path.display());
    }

    // Setup Configuration and Metrics ---------------------------------------------

    // Pull Env variables into config struct
    let config = setup_config(config_path);

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
    let task_runner_config = config
        .background_task_runner_config
        .clone()
        .unwrap_or_default();
    let bg_task_definitions: Vec<Box<dyn BgTask>> = vec![Box::new(DownloadMetadataTask {
        lock_duration: task_runner_config.lock_duration,
        max_attempts: task_runner_config.max_attempts,
        timeout: Some(time::Duration::from_secs(
            task_runner_config.timeout.unwrap_or(3),
        )),
    })];

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
    let mut timer_backfiller_acc = StreamSizeTimer::new(
        stream_metrics_timer,
        config.messenger_config.clone(),
        ACCOUNT_BACKFILL_STREAM,
    )?;
    let mut timer_txn = StreamSizeTimer::new(
        stream_metrics_timer,
        config.messenger_config.clone(),
        TRANSACTION_STREAM,
    )?;
    let mut timer_backfiller_txn = StreamSizeTimer::new(
        stream_metrics_timer,
        config.messenger_config.clone(),
        TRANSACTION_BACKFILL_STREAM,
    )?;

    if let Some(t) = timer_acc.start::<RedisMessenger>().await {
        tasks.spawn(t);
    }
    if let Some(t) = timer_backfiller_acc.start::<RedisMessenger>().await {
        tasks.spawn(t);
    }
    if let Some(t) = timer_txn.start::<RedisMessenger>().await {
        tasks.spawn(t);
    }
    if let Some(t) = timer_backfiller_txn.start::<RedisMessenger>().await {
        tasks.spawn(t);
    }

    // Stream Consumers Setup -------------------------------------
    if role == IngesterRole::Ingester || role == IngesterRole::All {
        let (_ack_task, ack_sender) =
            ack_worker::<RedisMessenger>(config.get_messneger_client_config());
        for i in 0..config.get_account_stream_worker_count() {
            let _account = account_worker::<RedisMessenger>(
                database_pool.clone(),
                config.get_messneger_client_config(),
                bg_task_sender.clone(),
                ack_sender.clone(),
                if i == 0 {
                    ConsumptionType::Redeliver
                } else {
                    ConsumptionType::New
                },
                ACCOUNT_STREAM,
            );
        }
        for i in 0..config.get_account_backfill_stream_worker_count() {
            let _account_backfill = account_worker::<RedisMessenger>(
                database_pool.clone(),
                config.get_messneger_client_config(),
                bg_task_sender.clone(),
                ack_sender.clone(),
                if i == 0 {
                    ConsumptionType::Redeliver
                } else {
                    ConsumptionType::New
                },
                ACCOUNT_BACKFILL_STREAM,
            );
        }
        for i in 0..config.get_transaction_stream_worker_count() {
            let _txn = transaction_worker::<RedisMessenger>(
                database_pool.clone(),
                config.get_messneger_client_config(),
                bg_task_sender.clone(),
                ack_sender.clone(),
                if i == 0 {
                    ConsumptionType::Redeliver
                } else {
                    ConsumptionType::New
                },
                config.cl_audits.unwrap_or(false),
                TRANSACTION_STREAM,
            );
        }
        for i in 0..config.get_transaction_backfill_stream_worker_count() {
            let _txn_backfill = transaction_worker::<RedisMessenger>(
                database_pool.clone(),
                config.get_messneger_client_config(),
                bg_task_sender.clone(),
                ack_sender.clone(),
                if i == 0 {
                    ConsumptionType::Redeliver
                } else {
                    ConsumptionType::New
                },
                config.cl_audits.unwrap_or(false),
                TRANSACTION_BACKFILL_STREAM,
            );
        }
    }
    // Stream Size Timers ----------------------------------------
    // Setup Stream Size Timers, these are small processes that run every 60 seconds and farm metrics for the size of the streams.
    // If metrics are disabled, these will not run.
    if role == IngesterRole::BackgroundTaskRunner || role == IngesterRole::All {
        let background_runner_config = config.clone().background_task_runner_config;
        tasks.spawn(background_task_manager.start_runner(background_runner_config));
    }
    // Backfiller Setup ------------------------------------------
    if role == IngesterRole::Backfiller || role == IngesterRole::All {
        let backfiller = setup_backfiller::<RedisMessenger>(database_pool.clone(), config.clone());
        tasks.spawn(backfiller);
    }

    let roles_str = role.to_string();
    metric! {
        statsd_count!("ingester.startup", 1, "role" => &roles_str, "version" => config.code_version.unwrap_or("unknown"));
    }
    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            error!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }

    tasks.shutdown().await;

    Ok(())
}
