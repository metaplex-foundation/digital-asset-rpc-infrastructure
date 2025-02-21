use anyhow::Result;
use borsh::BorshDeserialize;
use clap::Parser;
use das_core::{connect_db, PoolArgs};
use das_core::{Rpc, SolanaRpcArgs};
use digital_asset_types::dao::{asset, token_accounts, tokens};
use log::{debug, error};
use sea_orm::{entity::*, sea_query::Expr, SqlxPostgresConnector};
use sea_orm::{EntityTrait, PaginatorTrait, QueryFilter};
use solana_sdk::pubkey;
use solana_sdk::pubkey::Pubkey;
use tokio::sync::mpsc::unbounded_channel;
use tokio::task::JoinHandle;
use tokio::task::JoinSet;

pub const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

pub const TOKEN_2022_PROGRAM_ID: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

pub const MAX_GET_MULTIPLE_ACCOUNTS: usize = 100;

#[derive(Parser, Clone, Debug)]
pub struct ConfigArgs {
    /// The number of worker threads
    #[arg(long, env, default_value = "25")]
    pub workers: u64,
    /// The number of db entries to process in a single batch
    #[arg(long, env, default_value = "100")]
    pub batch_size: u64,
}

#[derive(Debug, Parser, Clone)]
pub struct Args {
    // The configuration for the purge command
    #[clap(flatten)]
    pub config: ConfigArgs,
    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,
    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,
}

pub async fn start_ta_purge(args: Args) -> Result<()> {
    let db_pool = connect_db(args.database).await?;

    let rpc = Rpc::from_config(args.solana);

    let (batch_sender, mut batch_receiver) = unbounded_channel::<Vec<Pubkey>>();

    let worker_count = args.config.workers as usize;

    let start = tokio::time::Instant::now();

    let pool = db_pool.clone();
    let control = tokio::spawn(async move {
        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

        let mut paginator = token_accounts::Entity::find().paginate(&conn, args.config.batch_size);

        while let Ok(Some(ta)) = paginator.fetch_and_next().await {
            let ta_keys = ta
                .iter()
                .filter_map(|ta| Pubkey::try_from_slice(ta.pubkey.as_slice()).ok())
                .collect::<Vec<Pubkey>>();

            if batch_sender.send(ta_keys).is_err() {
                error!("Failed to send mint keys");
            }
        }
    });

    let mut tasks = JoinSet::new();
    while let Some(ta_keys) = batch_receiver.recv().await {
        if tasks.len() >= worker_count {
            tasks.join_next().await;
        }
        tasks.spawn(fetch_and_purge_ta(db_pool.clone(), ta_keys, rpc.clone()));
    }

    control.await?;

    while tasks.join_next().await.is_some() {}

    debug!("Purge took: {:?}", start.elapsed());

    Ok(())
}

async fn fetch_and_purge_ta(pool: sqlx::PgPool, acc_keys: Vec<Pubkey>, rpc: Rpc) {
    let rpc = rpc.clone();

    let acc_keys_chuncks = acc_keys.chunks(MAX_GET_MULTIPLE_ACCOUNTS);

    let mut tasks = Vec::with_capacity(acc_keys_chuncks.len());

    for chunk in acc_keys_chuncks {
        debug!("chunk len: {:?}", chunk.len());
        let keys = chunk.to_vec();
        let pool = pool.clone();
        let rpc = rpc.clone();
        let handle = tokio::spawn(async move {
            if let Ok(accounts) = rpc.get_multiple_accounts(&keys).await {
                debug!("rpc fetched accounts len: {:?}", accounts.len());
                let mut accounts_to_purge = Vec::new();
                for (key, acc) in keys.iter().zip(accounts.iter()) {
                    match acc {
                        Some(acc) => {
                            if acc.owner.ne(&TOKEN_PROGRAM_ID)
                                && acc.owner.ne(&TOKEN_2022_PROGRAM_ID)
                            {
                                accounts_to_purge.push(key);
                            }
                        }
                        None => {
                            accounts_to_purge.push(key);
                        }
                    }
                }

                let accounts_to_purge = accounts_to_purge
                    .iter()
                    .map(|a| a.to_bytes().to_vec())
                    .collect::<Vec<Vec<u8>>>();

                debug!("accounts to purge len: {:?}", accounts_to_purge.len());

                if !accounts_to_purge.is_empty() {
                    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
                    let delete_res = token_accounts::Entity::delete_many()
                        .filter(
                            token_accounts::Column::Pubkey
                                .is_in(accounts_to_purge.iter().map(|a| a.as_slice())),
                        )
                        .exec(&conn)
                        .await;

                    if let Ok(res) = delete_res {
                        debug!(
                            "Successfully purged token accounts: {:?}",
                            res.rows_affected
                        );
                    } else {
                        error!("Failed to purge token accounts: {:?}", accounts_to_purge);
                    }
                }
            }
        });

        tasks.push(handle);
    }

    futures::future::join_all(tasks).await;
}

pub async fn start_mint_purge(args: Args) -> Result<()> {
    let db_pool = connect_db(args.database).await?;

    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(db_pool.clone());

    let rpc = Rpc::from_config(args.solana);

    let (batch_sender, mut batch_receiver) = unbounded_channel::<Vec<Pubkey>>();

    let worker_count = args.config.workers as usize;
    let start = tokio::time::Instant::now();

    let control = tokio::spawn(async move {
        let mut paginator = asset::Entity::find()
            .filter(asset::Column::Burnt.eq(false))
            .paginate(&conn, args.config.batch_size);

        while let Ok(Some(assets)) = paginator.fetch_and_next().await {
            let mint_keys = assets
                .iter()
                .filter_map(|a| Pubkey::try_from_slice(a.id.as_slice()).ok())
                .collect::<Vec<Pubkey>>();

            if batch_sender.send(mint_keys).is_err() {
                error!("Failed to send mint keys");
            }
        }
    });

    let mut tasks = JoinSet::new();

    while let Some(mint_keys) = batch_receiver.recv().await {
        if tasks.len() >= worker_count {
            tasks.join_next().await;
        }
        tasks.spawn(fetch_and_purge_assets(
            db_pool.clone(),
            mint_keys,
            rpc.clone(),
        ));
    }

    control.await?;

    while tasks.join_next().await.is_some() {}

    debug!("Purge took: {:?}", start.elapsed());

    Ok(())
}

fn fetch_and_purge_assets(pool: sqlx::PgPool, mint_keys: Vec<Pubkey>, rpc: Rpc) -> JoinHandle<()> {
    let rpc = rpc.clone();

    let handle = tokio::spawn(async move {
        let mint_keys_chuncks = mint_keys.chunks(MAX_GET_MULTIPLE_ACCOUNTS);

        let mut tasks = Vec::with_capacity(mint_keys_chuncks.len());

        for chunk in mint_keys_chuncks {
            debug!("chunk len: {:?}", chunk.len());
            let keys = chunk.to_vec();
            let pool = pool.clone();
            let rpc = rpc.clone();
            let handle = tokio::spawn(async move {
                if let Ok(accounts) = rpc.get_multiple_accounts(&keys).await {
                    debug!("rpc fetched accounts len: {:?}", accounts.len());
                    let mut accounts_to_update = Vec::new();
                    for (key, acc) in keys.iter().zip(accounts.iter()) {
                        match acc {
                            Some(acc) => {
                                if acc.owner.ne(&TOKEN_PROGRAM_ID)
                                    && acc.owner.ne(&TOKEN_2022_PROGRAM_ID)
                                {
                                    accounts_to_update.push(key);
                                }
                            }
                            None => {
                                accounts_to_update.push(key);
                            }
                        }
                    }

                    let accounts_to_update = accounts_to_update
                        .iter()
                        .map(|a| a.to_bytes().to_vec())
                        .collect::<Vec<Vec<u8>>>();

                    debug!("accounts to update len: {:?}", accounts_to_update.len());

                    if !accounts_to_update.is_empty() {
                        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());

                        let (update_res, delete_res) = tokio::join!(
                            asset::Entity::update_many()
                                .filter(
                                    asset::Column::Id
                                        .is_in(accounts_to_update.iter().map(|a| a.as_slice())),
                                )
                                .col_expr(asset::Column::Burnt, Expr::value(true))
                                .exec(&conn),
                            tokens::Entity::delete_many()
                                .filter(
                                    tokens::Column::Mint
                                        .is_in(accounts_to_update.iter().map(|a| a.as_slice())),
                                )
                                .exec(&conn)
                        );

                        if let Ok(res) = update_res {
                            debug!(
                                "Successfully marked assets as burnt: {:?}",
                                res.rows_affected
                            );
                        } else {
                            error!("Failed to update assets: {:?}", accounts_to_update);
                        }

                        if let Ok(res) = delete_res {
                            debug!("Successfully purged tokens: {:?}", res.rows_affected);
                        } else {
                            error!("Failed to purge tokens: {:?}", accounts_to_update);
                        }
                    }
                }
            });

            tasks.push(handle);
        }

        futures::future::join_all(tasks).await;
    });

    handle
}
