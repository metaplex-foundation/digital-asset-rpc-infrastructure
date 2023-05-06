use {
    clap::{value_parser, Arg, ArgAction, Command},
    digital_asset_types::dao::{asset_data, tasks},
    futures::TryStreamExt,
    log::{debug, info},
    nft_ingester::{
        config::rand_string,
        config::{init_logger, setup_config},
        database::setup_database,
        error::IngesterError,
        metrics::setup_metrics,
        tasks::{BgTask, DownloadMetadata, DownloadMetadataTask, IntoTaskData, TaskManager},
    },
    sea_orm::{entity::*, query::*, DeleteResult, EntityTrait, JsonValue, SqlxPostgresConnector},
    sqlx::types::chrono::Utc,
    std::{path::PathBuf, time},
    tokio::task::JoinSet,
};

/**
 * The bgtask creator is intended to be use as a tool to handle assets that have not been indexed.
 * It will delete all the current bgtasks and create new ones for assets where the metadata is missing.
 *
 * Currently it will try every missing asset every run.
 */

#[tokio::main(flavor = "multi_thread")]
pub async fn main() {
    init_logger();
    info!("Starting bgtask creator");

    let matches = Command::new("bgtaskcreator")
        .arg(
            Arg::new("config")
                .long("config")
                .short('c')
                .help("Sets a custom config file")
                .required(false)
                .action(ArgAction::Set)
                .value_parser(value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("delete")
                .long("delete")
                .short('d')
                .help("Delete all existing tasks before creating new ones.")
                .required(false),
        )
        .arg(
            Arg::new("batch_size")
                .long("batch-size")
                .short('b')
                .help("Sets the batch size for the assets to be processed.")
                .required(false)
                .action(ArgAction::Set)
                .value_parser(value_parser!(u64))
                .default_value("1000"),
        )
        .get_matches();

    let config_path = matches.get_one::<PathBuf>("config");
    if let Some(config_path) = config_path {
        println!("Loading config from: {}", config_path.display());
    }

    // Pull Env variables into config struct
    let config = setup_config(config_path);

    // Optionally setup metrics if config demands it
    setup_metrics(&config);

    // One pool many clones, this thing is thread safe and send sync
    let database_pool = setup_database(config.clone()).await;

    // Set up a task pool
    let mut tasks = JoinSet::new();

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
    let bg_task_listener = background_task_manager.start_listener(false);
    tasks.spawn(bg_task_listener);

    let bg_task_sender = background_task_manager.get_sender().unwrap();

    // Create new postgres connection
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(database_pool.clone());

    if matches.contains_id("delete") {
        info!("Deleting all existing tasks");

        // Delete all existing tasks
        let deleted_tasks: Result<DeleteResult, IngesterError> = tasks::Entity::delete_many()
            .exec(&conn)
            .await
            .map_err(|e| e.into());

        match deleted_tasks {
            Ok(result) => {
                info!("Deleted a number of tasks {}", result.rows_affected);
            }
            Err(e) => {
                info!("Error deleting tasks: {}", e);
            }
        }
    }

    let batch_size = matches.get_one::<u64>("batch_size").unwrap();

    info!(
        "Creating new tasks for assets with missing metadata, batch size={}",
        batch_size
    );

    // Find all the assets with missing metadata
    let mut asset_data_missing = asset_data::Entity::find()
        .filter(
            Condition::all()
                .add(asset_data::Column::Metadata.eq(JsonValue::String("processing".to_string()))),
        )
        .order_by(asset_data::Column::Id, Order::Asc)
        .paginate(&conn, *batch_size)
        .into_stream();

    while let Some(assets) = asset_data_missing.try_next().await.unwrap() {
        info!("Found {} assets", assets.len());
        for asset in assets {
            let mut task = DownloadMetadata {
                asset_data_id: asset.id,
                uri: asset.metadata_url,
                created_at: Some(Utc::now().naive_utc()),
            };

            debug!("Print task {}", task);
            task.sanitize();
            let task_data = task.into_task_data().unwrap();
            let _ = bg_task_sender.send(task_data);
        }
    }
}
