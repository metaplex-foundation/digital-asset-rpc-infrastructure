use {
    clap::{value_parser, Arg, ArgAction, Command},
    digital_asset_types::dao::{
        asset, asset_authority, asset_creators, asset_data, asset_grouping,
        sea_orm_active_enums::TaskStatus, tasks, tokens,
    },
    futures::TryStreamExt,
    log::{debug, error, info},
    nft_ingester::{
        config::{init_logger, rand_string, setup_config},
        database::setup_database,
        error::IngesterError,
        metrics::setup_metrics,
        tasks::{BgTask, DownloadMetadata, DownloadMetadataTask, IntoTaskData, TaskManager},
    },
    sea_orm::{
        entity::*, query::*, DbBackend, DeleteResult, EntityTrait, JsonValue, SqlxPostgresConnector,
    },
    solana_sdk::pubkey::Pubkey,
    sqlx::types::chrono::Utc,
    std::{collections::HashMap, path::PathBuf, str::FromStr, sync::Arc, time},
};

/**
 * The bgtask creator is intended to be use as a tool to handle assets that have not been indexed.
 * It will delete all the current bgtasks and create new ones for assets where the metadata is missing.
 *
 * Currently it will try every missing asset every run.
 */

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
            Arg::new("batch_size")
                .long("batch-size")
                .short('b')
                .help("Sets the batch size for the assets to be processed.")
                .required(false)
                .action(ArgAction::Set)
                .value_parser(value_parser!(u64))
                .default_value("1000"),
        )
        .arg(
            Arg::new("authority")
                .long("authority")
                .short('a')
                .help("Create/show background tasks for the given authority")
                .required(false)
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("collection")
                .long("collection")
                .short('o')
                .help("Create/show background tasks for the given collection")
                .required(false)
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("mint")
                .long("mint")
                .short('m')
                .help("Create/show background tasks for the given mint")
                .required(false)
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("creator")
                .long("creator")
                .short('r')
                .help("Create/show background tasks for the given creator")
                .required(false)
                .action(ArgAction::Set),
        )
        .subcommand(
            Command::new("show").about("Show tasks").arg(
                Arg::new("print")
                    .long("print")
                    .short('p')
                    .help("Print the tasks to stdout")
                    .required(false)
                    .action(clap::ArgAction::SetTrue),
            ),
        )
        .subcommand(
            Command::new("reindex").about("Set reindex=true on all assets where metadata=pending"),
        )
        .subcommand(
            Command::new("create")
                .about("Create new background tasks for missing assets (reindex=true)"),
        )
        .subcommand(Command::new("delete").about("Delete ALL pending background tasks"))
        .get_matches();

    let config_path = matches.get_one::<PathBuf>("config");
    if let Some(config_path) = config_path {
        info!("Loading config from: {}", config_path.display());
    }

    // Pull Env variables into config struct
    let config = setup_config(config_path);

    // Optionally setup metrics if config demands it
    setup_metrics(&config);

    // One pool many clones, this thing is thread safe and send sync
    let database_pool = setup_database(config.clone()).await;

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
    let mut bg_tasks = HashMap::new();
    for task in bg_task_definitions {
        bg_tasks.insert(task.name().to_string(), task);
    }
    let task_map = Arc::new(bg_tasks);

    let instance_name = rand_string();

    // Get a postgres connection from the pool
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(database_pool.clone());

    let batch_size = matches.get_one::<u64>("batch_size").unwrap();
    let authority = matches.get_one::<String>("authority");
    let collection = matches.get_one::<String>("collection");
    let mint = matches.get_one::<String>("mint");
    let creator = matches.get_one::<String>("creator");

    /*
           select ad.id from asset_data ad
    inner join asset_authority aa  on aa.asset_id = ad.id
    where
     aa.authority='\x0b6eeb8809df3468cbe2ee7b224e7b3291d99770811728fcdefbc180c6933157' and
     ad.metadata=to_jsonb('processing'::text);
      */

    match matches.subcommand_name() {
        Some("reindex") => {
            let exec_res = conn
                .execute(Statement::from_string(
                    DbBackend::Postgres,
                    "
UPDATE
    asset_data
SET
    reindex = TRUE
WHERE
    asset_data.metadata = to_jsonb('processing'::text) AND
    reindex = FALSE
"
                    .to_owned(),
                ))
                .await;
            info!("Updated {:?} assets", exec_res.unwrap().rows_affected());
        }
        Some("show") => {
            // Check the total number of assets in the DB
            let condition_found =
                asset_data::Column::Metadata.ne(JsonValue::String("processing".to_string()));
            let condition_missing =
                asset_data::Column::Metadata.eq(JsonValue::String("processing".to_string()));
            let condition_reindex = asset_data::Column::Reindex.eq(true);

            let asset_data_finished =
                find_by_type(authority, collection, creator, mint, condition_found);
            let asset_data_processing =
                find_by_type(authority, collection, creator, mint, condition_missing);
            let asset_data_reindex =
                find_by_type(authority, collection, creator, mint, condition_reindex);

            let mut asset_data_missing = asset_data_processing
                .0
                .order_by(asset_data::Column::Id, Order::Asc)
                .paginate(&conn, *batch_size)
                .into_stream();

            let asset_data_count = asset_data_finished.0.count(&conn).await;
            let asset_reindex_count = asset_data_reindex.0.count(&conn).await;

            let mut i = 0;
            while let Some(assets) = asset_data_missing.try_next().await.unwrap() {
                info!("Found {} assets", assets.len());
                i += assets.len();
                if let Some(matches) = matches.subcommand_matches("show") {
                    if matches.get_flag("print") {
                        for asset in assets {
                            println!(
                                "{}, missing asset, {:?}",
                                asset_data_processing.1,
                                Pubkey::try_from(asset.id)
                            );
                        }
                    }
                }
            }

            let total_finished = asset_data_count.unwrap_or(0);
            let total_assets = i + total_finished as usize;
            println!("{}, reindexing assets: {:?}, total finished assets: {}, missing assets: {}, total assets: {}", asset_data_processing.1, asset_reindex_count, total_finished, i, total_assets);
        }
        Some("delete") => {
            println!("Deleting all existing tasks");

            // Delete all existing tasks
            let deleted_tasks: Result<DeleteResult, IngesterError> = tasks::Entity::delete_many()
                .exec(&conn)
                .await
                .map_err(|e| e.into());

            match deleted_tasks {
                Ok(result) => {
                    println!("Deleted a number of tasks {}", result.rows_affected);
                }
                Err(e) => {
                    println!("Error deleting tasks: {}", e);
                }
            }
        }
        Some("create") => {
            // @TODO : add a delete option that first deletes all matching tasks to the criteria or condition

            let condition = asset_data::Column::Reindex.eq(true);
            let asset_data = find_by_type(authority, collection, creator, mint, condition);

            let mut asset_data_missing = asset_data
                .0
                .order_by(asset_data::Column::Id, Order::Asc)
                .paginate(&conn, *batch_size)
                .into_stream();

            // Find all the assets with missing metadata
            let mut tasks = Vec::new();
            while let Some(assets) = asset_data_missing.try_next().await.unwrap() {
                println!("Total missing {} assets", assets.len());
                for asset in assets {
                    let mut task = DownloadMetadata {
                        asset_data_id: asset.id,
                        uri: asset.metadata_url,
                        created_at: Some(Utc::now().naive_utc()),
                    };

                    task.sanitize();
                    let task_data = task.clone().into_task_data().unwrap();

                    debug!("Print task {} hash {:?}", task_data.data, task_data.hash());
                    let name = instance_name.clone();
                    if let Ok(hash) = task_data.hash() {
                        let database_pool = database_pool.clone();
                        let task_map = task_map.clone();
                        let name = name.clone();
                        let new_task = tokio::task::spawn(async move {
                            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(
                                database_pool.clone(),
                            );

                            // Check if the task being added is already stored in the DB and is not pending
                            let task_entry = tasks::Entity::find_by_id(hash.clone())
                                .filter(tasks::Column::Status.ne(TaskStatus::Pending))
                                .one(&conn)
                                .await;
                            if let Ok(Some(e)) = task_entry {
                                debug!("Found duplicate task: {:?} {:?}", e, hash.clone());
                                return;
                            }

                            let task_hash = task_data.hash();
                            info!("Created task: {:?}", task_hash);

                            let res = TaskManager::new_task_handler(
                                database_pool.clone(),
                                name.clone(),
                                name,
                                task_data,
                                task_map.clone(),
                                false,
                            )
                            .await;

                            match res {
                                Ok(_) => {
                                    info!(
                                        "Task completed: {:?} {:?}",
                                        task_hash, task.asset_data_id
                                    );
                                }
                                Err(e) => {
                                    error!("Task failed: {}", e);
                                }
                            }
                        });
                        tasks.push(new_task);
                    }
                }
            }

            if tasks.is_empty() {
                println!("No assets with missing metadata found");
            } else {
                println!("Found {} tasks to process", tasks.len());
                let mut succeeded = 0;
                let mut failed = 0;
                for task in tasks {
                    match task.await {
                        Ok(_) => succeeded += 1,
                        Err(e) => {
                            println!("Task failed: {}", e);
                            failed += 1;
                        }
                    }
                }
                println!("Tasks succeeded={}, failed={}", succeeded, failed);
            }
        }
        _ => {
            println!("Please provide an action")
        }
    }
}

fn find_by_type<'a>(
    authority: Option<&'a String>,
    collection: Option<&'a String>,
    creator: Option<&'a String>,
    mint: Option<&'a String>,
    condition: sea_orm::sea_query::SimpleExpr,
) -> (
    sea_orm::Select<digital_asset_types::dao::asset_data::Entity>,
    String,
) {
    if let Some(authority) = authority {
        info!(
            "Find asset data for authority {} condition {:?}",
            authority, condition
        );

        let pubkey = Pubkey::from_str(authority.as_str()).unwrap();
        let pubkey_bytes = pubkey.to_bytes().to_vec();

        (
            asset_data::Entity::find()
                .join_rev(
                    JoinType::InnerJoin,
                    asset_authority::Entity::belongs_to(asset_data::Entity)
                        .from(asset_authority::Column::AssetId)
                        .to(asset_data::Column::Id)
                        .into(),
                )
                .filter(
                    Condition::all()
                        .add(asset_authority::Column::Authority.eq(pubkey_bytes))
                        .add(condition),
                ),
            authority.to_string(),
        )
    } else if let Some(collection) = collection {
        info!(
            "Finding asset_data for collection {}, condition {:?}",
            collection, condition
        );

        (
            asset_data::Entity::find()
                .join_rev(
                    JoinType::InnerJoin,
                    asset_grouping::Entity::belongs_to(asset_data::Entity)
                        .from(asset_grouping::Column::AssetId)
                        .to(asset_data::Column::Id)
                        .into(),
                )
                .filter(
                    Condition::all()
                        .add(asset_grouping::Column::GroupValue.eq(collection.as_str()))
                        .add(condition),
                ),
            collection.to_string(),
        )
    } else if let Some(mint) = mint {
        info!(
            "Finding assets for mint {}, condition {:?}",
            mint, condition
        );

        let pubkey = Pubkey::from_str(mint.as_str()).unwrap();
        let pubkey_bytes = pubkey.to_bytes().to_vec();

        (
            asset_data::Entity::find()
                .join(JoinType::InnerJoin, asset_data::Relation::Asset.def())
                .join_rev(
                    JoinType::InnerJoin,
                    tokens::Entity::belongs_to(asset::Entity)
                        .from(tokens::Column::Mint)
                        .to(asset::Column::SupplyMint)
                        .into(),
                )
                .filter(
                    Condition::all()
                        .add(tokens::Column::MintAuthority.eq(pubkey_bytes))
                        .add(condition),
                ),
            mint.to_string(),
        )
    } else if let Some(creator) = creator {
        info!(
            "Finding assets for creator {} with condition {:?}",
            creator, condition
        );

        let pubkey = Pubkey::from_str(creator.as_str()).unwrap();
        let pubkey_bytes = pubkey.to_bytes().to_vec();

        (
            asset_data::Entity::find()
                .join_rev(
                    JoinType::InnerJoin,
                    asset_creators::Entity::belongs_to(asset_data::Entity)
                        .from(asset_creators::Column::AssetId)
                        .to(asset_data::Column::Id)
                        .into(),
                )
                .filter(
                    Condition::all()
                        .add(asset_creators::Column::Creator.eq(pubkey_bytes))
                        .add(condition),
                ),
            creator.to_string(),
        )
    } else {
        info!("Finding all assets with condition {:?}", condition,);
        (
            asset_data::Entity::find().filter(Condition::all().add(condition)),
            "all".to_string(),
        )
    }
}
