use anyhow::Result;
use borsh::BorshDeserialize;
use clap::Parser;
use das_core::Rpc;
use das_core::{DbConn, DbPool};
use digital_asset_types::dao::{asset, token_accounts, tokens};
use log::{debug, error};
use sea_orm::{entity::*, sea_query::Expr};
use sea_orm::{DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter};

use solana_sdk::{bs58, pubkey};

use solana_sdk::pubkey::Pubkey;

use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;
use tokio::task::JoinSet;

pub const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

pub const TOKEN_2022_PROGRAM_ID: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

pub const MAX_GET_MULTIPLE_ACCOUNTS: usize = 100;

#[derive(Debug, Clone)]
pub enum PurgeTable {
    TokenAccounts,
    Tokens,
}

pub struct DbEntriesBatchSender {
    pub db: DatabaseConnection,
    pub purge_table: PurgeTable,
}

impl DbEntriesBatchSender {
    pub fn new(db: DatabaseConnection, purge_table: PurgeTable) -> Self {
        Self { db, purge_table }
    }
}

pub struct RpcBatchFetcherAndPurger {
    pub rpc: Rpc,
    pub db: DatabaseConnection,
    pub purge_table: PurgeTable,
}

impl RpcBatchFetcherAndPurger {
    pub fn new(rpc: Rpc, db: DatabaseConnection, purge_table: PurgeTable) -> Self {
        Self {
            rpc,
            db,
            purge_table,
        }
    }
}

pub trait PurgeHandler<I, O> {
    fn handle(self, input: I) -> O;
}

impl PurgeHandler<u64, (JoinHandle<()>, UnboundedReceiver<Vec<Pubkey>>)> for DbEntriesBatchSender {
    fn handle(self, batch_size: u64) -> (JoinHandle<()>, UnboundedReceiver<Vec<Pubkey>>) {
        let (batch_sender, batch_receiver) = unbounded_channel::<Vec<Pubkey>>();
        let purge_table = self.purge_table.clone();

        let db = self.db;
        let control = tokio::spawn(async move {
            match purge_table {
                PurgeTable::TokenAccounts => {
                    let mut paginator = token_accounts::Entity::find().paginate(&db, batch_size);
                    while let Ok(Some(ta)) = paginator.fetch_and_next().await {
                        let ta_keys = ta
                            .iter()
                            .filter_map(|ta| Pubkey::try_from_slice(ta.pubkey.as_slice()).ok())
                            .collect::<Vec<Pubkey>>();

                        if batch_sender.send(ta_keys).is_err() {
                            error!("Failed to send mint keys");
                        }
                    }
                }
                PurgeTable::Tokens => {
                    let mut paginator = tokens::Entity::find().paginate(&db, batch_size);
                    while let Ok(Some(ta)) = paginator.fetch_and_next().await {
                        let ta_keys = ta
                            .iter()
                            .filter_map(|ta| Pubkey::try_from_slice(ta.mint.as_slice()).ok())
                            .collect::<Vec<Pubkey>>();

                        if batch_sender.send(ta_keys).is_err() {
                            error!("Failed to send mint keys");
                        }
                    }
                }
            }
        });

        (control, batch_receiver)
    }
}

impl PurgeHandler<Vec<Pubkey>, JoinHandle<()>> for RpcBatchFetcherAndPurger {
    fn handle(self, input: Vec<Pubkey>) -> JoinHandle<()> {
        let control = tokio::spawn(async move {
            if let Ok(accounts) = self.rpc.get_multiple_accounts(&input).await {
                let mut accounts_to_purge = Vec::new();
                for (key, acc) in input.iter().zip(accounts.iter()) {
                    match acc {
                        Some(acc) => {
                            if acc.owner.ne(&TOKEN_PROGRAM_ID)
                                && acc.owner.ne(&TOKEN_2022_PROGRAM_ID)
                            {
                                accounts_to_purge.push(*key);
                            }
                        }
                        None => {
                            accounts_to_purge.push(*key);
                        }
                    }
                }

                let accounts_to_purge = accounts_to_purge
                    .iter()
                    .map(|a| a.to_bytes().to_vec())
                    .collect::<Vec<Vec<u8>>>();

                debug!("accounts to purge len: {:?}", accounts_to_purge.len());

                if !accounts_to_purge.is_empty() {
                    match self.purge_table {
                        PurgeTable::TokenAccounts => {
                            let delete_res = token_accounts::Entity::delete_many()
                                .filter(
                                    token_accounts::Column::Pubkey
                                        .is_in(accounts_to_purge.iter().map(|a| a.as_slice())),
                                )
                                .exec(&self.db)
                                .await;

                            if let Ok(res) = delete_res {
                                debug!(
                                    "Successfully purged token accounts: {:?}",
                                    res.rows_affected
                                );
                            } else {
                                let accounts_to_purge = accounts_to_purge
                                    .iter()
                                    .map(|a| bs58::encode(a).into_string())
                                    .collect::<Vec<String>>();
                                error!("Failed to purge token accounts: {:?}", accounts_to_purge);
                            }
                        }
                        PurgeTable::Tokens => {
                            let (delete_res, update_res) = tokio::join!(
                                tokens::Entity::delete_many()
                                    .filter(
                                        tokens::Column::Mint
                                            .is_in(accounts_to_purge.iter().map(|a| a.as_slice())),
                                    )
                                    .exec(&self.db),
                                asset::Entity::update_many()
                                    .filter(
                                        asset::Column::Id
                                            .is_in(accounts_to_purge.iter().map(|a| a.as_slice()))
                                            .and(asset::Column::Burnt.eq(false),),
                                    )
                                    .col_expr(asset::Column::Burnt, Expr::value(true))
                                    .exec(&self.db),
                            );

                            let accounts_to_purge = accounts_to_purge
                                .iter()
                                .map(|a| bs58::encode(a).into_string())
                                .collect::<Vec<String>>();
                            if let Ok(res) = update_res {
                                debug!(
                                    "Successfully marked assets as burnt: {:?}",
                                    res.rows_affected
                                );
                            } else {
                                error!("Failed to update assets: {:?}", accounts_to_purge);
                            }

                            if let Ok(res) = delete_res {
                                debug!("Successfully purged tokens: {:?}", res.rows_affected);
                            } else {
                                error!("Failed to purge tokens: {:?}", accounts_to_purge);
                            }
                        }
                    }
                }
            }
        });

        control
    }
}

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
}

pub async fn start_ta_purge<P: DbConn>(args: Args, db_pool: DbPool<P>, rpc: Rpc) -> Result<()> {
    let worker_count = args.config.workers as usize;

    let start = tokio::time::Instant::now();

    let conn = db_pool.get_db_conn();

    let db_batch_sender = DbEntriesBatchSender::new(conn, PurgeTable::TokenAccounts);

    let (control, mut batch_receiver) = db_batch_sender.handle(args.config.batch_size);

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

async fn fetch_and_purge_ta<P: DbConn>(pool: DbPool<P>, acc_keys: Vec<Pubkey>, rpc: Rpc) {
    let acc_keys_chuncks = acc_keys.chunks(MAX_GET_MULTIPLE_ACCOUNTS);

    let mut tasks = Vec::with_capacity(acc_keys_chuncks.len());

    for chunk in acc_keys_chuncks {
        debug!("chunk len: {:?}", chunk.len());
        let keys = chunk.to_vec();
        let conn = pool.get_db_conn();
        let rpc_batch_fetcher_and_sender =
            RpcBatchFetcherAndPurger::new(rpc.clone(), conn, PurgeTable::TokenAccounts);

        let control = rpc_batch_fetcher_and_sender.handle(keys);

        tasks.push(control);
    }

    futures::future::join_all(tasks).await;
}

pub async fn start_mint_purge<P: DbConn>(args: Args, db_pool: DbPool<P>, rpc: Rpc) -> Result<()> {
    let worker_count = args.config.workers as usize;

    let start = tokio::time::Instant::now();

    let conn = db_pool.get_db_conn();

    let db_batch_sender = DbEntriesBatchSender::new(conn, PurgeTable::Tokens);

    let (control, mut batch_receiver) = db_batch_sender.handle(args.config.batch_size);

    let mut tasks = JoinSet::new();
    while let Some(ta_keys) = batch_receiver.recv().await {
        if tasks.len() >= worker_count {
            tasks.join_next().await;
        }
        tasks.spawn(fetch_and_purge_assets(
            db_pool.clone(),
            ta_keys,
            rpc.clone(),
        ));
    }

    control.await?;

    while tasks.join_next().await.is_some() {}

    debug!("Purge took: {:?}", start.elapsed());

    Ok(())
}

async fn fetch_and_purge_assets<P: DbConn>(pool: DbPool<P>, acc_keys: Vec<Pubkey>, rpc: Rpc) {
    let acc_keys_chuncks = acc_keys.chunks(MAX_GET_MULTIPLE_ACCOUNTS);

    let mut tasks = Vec::with_capacity(acc_keys_chuncks.len());

    for chunk in acc_keys_chuncks {
        debug!("chunk len: {:?}", chunk.len());
        let keys = chunk.to_vec();

        let conn = pool.get_db_conn();

        let rpc_batch_fetcher_and_sender =
            RpcBatchFetcherAndPurger::new(rpc.clone(), conn, PurgeTable::Tokens);

        let control = rpc_batch_fetcher_and_sender.handle(keys);

        tasks.push(control);
    }

    futures::future::join_all(tasks).await;
}
