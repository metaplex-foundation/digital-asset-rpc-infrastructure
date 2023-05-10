use digital_asset_types::dao::{sea_orm_active_enums::TaskStatus, asset_data, asset_authority, asset_grouping, asset_creators, tokens, tasks, asset};

use log::{info, debug, error};

use nft_ingester::{
    tasks::{BgTask, DownloadMetadata, IntoTaskData, DownloadMetadataTask, TaskManager},
    config::{init_logger, setup_config},
    database::setup_database,
    metrics::setup_metrics,
    config::rand_string,
    error::IngesterError,
};

use std::{
    path::PathBuf,
    time
};

use futures::TryStreamExt;

use sea_orm::{
    entity::*, query::*, EntityTrait, JsonValue, SqlxPostgresConnector, DeleteResult
};

use clap::{Arg, ArgAction, Command, value_parser};

use sqlx::types::chrono::Utc;

use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, collections::HashMap, sync::Arc};

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
                .required(false)
                .action(clap::ArgAction::SetTrue)
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
                .help("Create background tasks for the given authority")
                .required(false)
                .action(ArgAction::Set)
        )
        .arg(
            Arg::new("collection")
                .long("collection")
                .short('o')
                .help("Create background tasks for the given collection")
                .required(false)
                .action(ArgAction::Set)
        )
        .arg(
            Arg::new("mint")
                .long("mint")
                .short('m')
                .help("Create background tasks for the given mint")
                .required(false)
                .action(ArgAction::Set)
        )
        .arg(
            Arg::new("creator")
                .long("creator")
                .short('r')
                .help("Create background tasks for the given creator")
                .required(false)
                .action(ArgAction::Set)
        )
        .subcommand(
            Command::new("show")
                .about("Show tasks")
                .arg(
                    Arg::new("print")
                        .long("print")
                        .short('p')
                        .help("Print the tasks to stdout")
                        .required(false)
                        .action(clap::ArgAction::SetTrue)
                )
        )
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

    if matches.get_flag("delete") {
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

    let all = "all".to_string();
    let mut asset_data_missing =  if let Some(authority) = authority {
            info!("Creating new tasks for assets with missing metadata for authority {}, batch size={}", authority, batch_size);

            let pubkey = Pubkey::from_str(&authority.as_str()).unwrap();
            let pubkey_bytes = pubkey.to_bytes().to_vec();
            
            (asset_data::Entity::find()
                 .join_rev(
                    JoinType::InnerJoin,
                    asset_authority::Entity::belongs_to(asset_data::Entity)
                        .from(asset_authority::Column::AssetId)
                        .to(asset_data::Column::Id)
                        .into()
                 )
                 .filter(
                    Condition::all()
                        .add(asset_authority::Column::Authority.eq(pubkey_bytes))
                        .add(asset_data::Column::Reindex.eq(false))
                 )
                 .order_by(asset_data::Column::Id, Order::Asc)
                .paginate(&conn, *batch_size)
                .into_stream(), authority)

        } else if let Some(collection) = collection {
            info!("Creating new tasks for assets with missing metadata for collection {}, batch size={}", collection, batch_size);

            (asset_data::Entity::find()
                 .join_rev(
                    JoinType::InnerJoin,
                    asset_grouping::Entity::belongs_to(asset_data::Entity)
                        .from(asset_grouping::Column::AssetId)
                        .to(asset_data::Column::Id)
                        .into()
                 )
                 .filter(
                    Condition::all()
                        .add(asset_grouping::Column::GroupValue.eq(collection.as_str()))
                        .add(asset_data::Column::Reindex.eq(false))
                 )
                 .order_by(asset_data::Column::Id, Order::Asc)
                .paginate(&conn, *batch_size)
                .into_stream(), collection)
        } else if let Some(mint) = mint {
            info!("Creating new tasks for assets with missing metadata for mint {}, batch size={}", mint, batch_size);
            
            let pubkey = Pubkey::from_str(&mint.as_str()).unwrap();
            let pubkey_bytes = pubkey.to_bytes().to_vec();

            (asset_data::Entity::find()
                 .join(
                    JoinType::InnerJoin,
                    asset::Relation::AssetData.def()
                 )
                 .join_rev(
                    JoinType::InnerJoin,
                    tokens::Entity::belongs_to(asset::Entity)
                        .from(tokens::Column::Mint)
                        .to(asset::Column::SupplyMint)
                        .into()
                 )
                 .filter(
                    Condition::all()
                        .add(tokens::Column::MintAuthority.eq(pubkey_bytes))
                        .add(asset_data::Column::Reindex.eq(false))
                 )
                 .order_by(asset_data::Column::Id, Order::Asc)
                .paginate(&conn, *batch_size)
                .into_stream(), mint)
        } else if let Some(creator) = creator {
            info!("Creating new tasks for assets with missing metadata for creator {}, batch size={}", creator, batch_size);

            let pubkey = Pubkey::from_str(&creator.as_str()).unwrap();
            let pubkey_bytes = pubkey.to_bytes().to_vec();

            (asset_data::Entity::find()
                 .join_rev(
                    JoinType::InnerJoin,
                    asset_creators::Entity::belongs_to(asset_data::Entity)
                        .from(asset_creators::Column::AssetId)
                        .to(asset_data::Column::Id)
                        .into()
                 )
                 .filter(
                    Condition::all()
                        .add(asset_creators::Column::Creator.eq(pubkey_bytes))
                        .add(asset_data::Column::Reindex.eq(false))
                 )
                .order_by(asset_data::Column::Id, Order::Asc)
                .paginate(&conn, *batch_size)
                .into_stream(), creator)
        } else {
            info!("Creating new tasks for all assets with missing metadata, batch size={}", batch_size);
            (asset_data::Entity::find()
                .filter(
                    Condition::all()
                        .add(asset_data::Column::Metadata.eq(JsonValue::String("processing".to_string())))
                )
                .order_by(asset_data::Column::Id, Order::Asc)
                .paginate(&conn, *batch_size)
                .into_stream(), &all)
        };
    
    let mut tasks = Vec::new();
    match matches.subcommand_name() {
        Some("show") => {
            // Check the assets found
            let asset_data_found = if let Some(authority) = authority {
                let pubkey = Pubkey::from_str(&authority.as_str()).unwrap();
                let pubkey_bytes = pubkey.to_bytes().to_vec();
                
                asset_data::Entity::find()
                    .join_rev(
                        JoinType::InnerJoin,
                        asset_authority::Entity::belongs_to(asset_data::Entity)
                            .from(asset_authority::Column::AssetId)
                            .to(asset_data::Column::Id)
                            .into()
                    )
                    .filter(
                        Condition::all()
                            .add(asset_authority::Column::Authority.eq(pubkey_bytes))
                            .add(asset_data::Column::Metadata.ne(JsonValue::String("processing".to_string())))
                    )
                    .count(&conn)
                    .await
            } else if let Some(collection) = collection {
                asset_data::Entity::find()
                    .join_rev(
                        JoinType::InnerJoin,
                        asset_grouping::Entity::belongs_to(asset_data::Entity)
                            .from(asset_grouping::Column::AssetId)
                            .to(asset_data::Column::Id)
                            .into()
                    )
                    .filter(
                        Condition::all()
                            .add(asset_grouping::Column::GroupValue.eq(collection.as_str()))
                            .add(asset_data::Column::Metadata.ne(JsonValue::String("processing".to_string())))
                    )
                    .count(&conn)
                    .await
            } else if let Some(mint) = mint {
                let pubkey = Pubkey::from_str(&mint.as_str()).unwrap();
                let pubkey_bytes = pubkey.to_bytes().to_vec();

                asset_data::Entity::find()
                    .join(
                        JoinType::InnerJoin,
                        asset::Relation::AssetData.def()
                    )
                    .join_rev(
                        JoinType::InnerJoin,
                        tokens::Entity::belongs_to(asset::Entity)
                            .from(tokens::Column::Mint)
                            .to(asset::Column::SupplyMint)
                            .into()
                    )
                    .filter(
                        Condition::all()
                            .add(tokens::Column::MintAuthority.eq(pubkey_bytes))
                            .add(asset_data::Column::Metadata.ne(JsonValue::String("processing".to_string())))
                    )
                    .count(&conn)
                    .await
            } else if let Some(creator) = creator {
                let pubkey = Pubkey::from_str(&creator.as_str()).unwrap();
                let pubkey_bytes = pubkey.to_bytes().to_vec();

                asset_data::Entity::find()
                    .join_rev(
                        JoinType::InnerJoin,
                        asset_creators::Entity::belongs_to(asset_data::Entity)
                            .from(asset_creators::Column::AssetId)
                            .to(asset_data::Column::Id)
                            .into()
                    )
                    .filter(
                        Condition::all()
                            .add(asset_creators::Column::Creator.eq(pubkey_bytes))
                            .add(asset_data::Column::Metadata.ne(JsonValue::String("processing".to_string())))
                    )
                    .count(&conn)
                    .await
            } else {
                asset_data::Entity::find()
                    .filter(
                        Condition::all()
                            .add(asset_data::Column::Metadata.ne(JsonValue::String("processing".to_string())))
                    )
                    .count(&conn)
                    .await
            };

            let mut i = 0;
            while let Some(assets) = asset_data_missing.0.try_next().await.unwrap() {
                info!("Found {} assets", assets.len());
                i += assets.len();
                if let Some(matches) = matches.subcommand_matches("show") {
                    if matches.get_flag("print") {
                        for asset in assets {
                            println!("{}, missing asset, {:?}", asset_data_missing.1, Pubkey::try_from(asset.id));
                        }
                    }
                }
            }
            if let Ok(total) = asset_data_found {
                println!("{}, total assets, {}", asset_data_missing.1, total);
            }
            println!("{}, total missing assets, {}", asset_data_missing.1, i)
        }
        _ => {
            // Find all the assets with missing metadata
            while let Some(assets) = asset_data_missing.0.try_next().await.unwrap() {
                    info!("Found {} assets", assets.len());
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
                                let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(database_pool.clone());

                                // Check if the task being added is already stored in the DB and is not pending
                                let task_entry = tasks::Entity::find_by_id(hash.clone())
                                    .filter(tasks::Column::Status.ne(TaskStatus::Pending))
                                    .one(&conn)
                                    .await;
                                if let Ok(Some(e)) = task_entry {
                                    debug!("Found duplicate task: {:?} {:?}", e, hash.clone());
                                    return
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
                                ).await;

                                match res {
                                    Ok(_) => {
                                        info!("Task completed: {:?} {:?}", task_hash, task.asset_data_id);
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
                info!("No assets with missing metadata found");
            } else {
                info!("Found {} tasks to process", tasks.len());
                for task in tasks {
                    let res = task.await; 
                    match res {
                        Ok(_) => {
                        }
                        Err(e) => {
                            error!("Task failed: {}", e);
                        }
                    }
                }
            }
        }
    }
}
