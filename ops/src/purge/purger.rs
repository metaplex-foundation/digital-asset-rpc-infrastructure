use anyhow::Result;
use borsh::BorshDeserialize;
use clap::Parser;
use das_core::{DbConn, DbPool, Rpc};
use digital_asset_types::dao::{token_accounts, tokens};
use log::{debug, error};
use sea_orm::FromQueryResult;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use std::marker::{Send, Sync};

use solana_sdk::{bs58, pubkey};

use solana_sdk::pubkey::Pubkey;

use sea_orm::QuerySelect;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;

pub const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

pub const TOKEN_2022_PROGRAM_ID: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

pub const MAX_GET_MULTIPLE_ACCOUNTS: usize = 100;

// Define a trait for accessing a specific column
trait ColumnAccess {
    type ColumnType: ColumnTrait;
    fn column() -> Self::ColumnType;
}

// Implement the trait for specific columns
impl ColumnAccess for token_accounts::Entity {
    type ColumnType = token_accounts::Column;

    fn column() -> Self::ColumnType {
        token_accounts::Column::Pubkey
    }
}

impl ColumnAccess for tokens::Entity {
    type ColumnType = tokens::Column;

    fn column() -> Self::ColumnType {
        tokens::Column::Mint
    }
}

struct Paginate<E: EntityTrait + ColumnAccess, P: DbConn> {
    pool: DbPool<P>,
    batch_size: u64,
    sender: UnboundedSender<Vec<Pubkey>>,
    _e_type: std::marker::PhantomData<E>,
}

#[derive(FromQueryResult)]
struct PubkeyResult {
    pubkey: Vec<u8>,
}

impl<E: EntityTrait + ColumnAccess, P: DbConn + Send> Paginate<E, P> {
    fn new(pool: DbPool<P>, batch_size: u64, sender: UnboundedSender<Vec<Pubkey>>) -> Self {
        Self {
            pool,
            batch_size,
            sender,
            _e_type: std::marker::PhantomData,
        }
    }

    async fn page(&self) {
        let conn = self.pool.connection();
        let mut paginator = E::find()
            .column_as(E::column(), "pubkey")
            .into_model::<PubkeyResult>()
            .paginate(&conn, self.batch_size);

        while let Ok(Some(records)) = paginator.fetch_and_next().await {
            let keys = records
                .iter()
                .filter_map(|row| Pubkey::try_from_slice(&row.pubkey).ok())
                .collect::<Vec<Pubkey>>();

            if self.sender.send(keys).is_err() {
                error!("Failed to send keys");
            }
        }
    }
}

struct MarkDeletion {
    rpc: Rpc,
    receiver: UnboundedReceiver<Vec<Pubkey>>,
    sender: UnboundedSender<Vec<Pubkey>>,
}

impl MarkDeletion {
    fn new(
        rpc: Rpc,
        receiver: UnboundedReceiver<Vec<Pubkey>>,
        sender: UnboundedSender<Vec<Pubkey>>,
    ) -> Self {
        Self {
            rpc,
            receiver,
            sender,
        }
    }

    async fn mark(&mut self) {
        while let Some(keys) = self.receiver.recv().await {
            for chunk in keys.chunks(MAX_GET_MULTIPLE_ACCOUNTS) {
                let rpc = self.rpc.clone();
                let sender = self.sender.clone();
                let chunk = chunk.to_vec();

                tokio::spawn(async move {
                    if let Ok(accounts) = rpc.get_multiple_accounts(&chunk).await {
                        let mut remove = Vec::new();

                        for (key, acc) in chunk.iter().zip(accounts.iter()) {
                            match acc {
                                Some(acc) => {
                                    if acc.owner.ne(&TOKEN_PROGRAM_ID)
                                        && acc.owner.ne(&TOKEN_2022_PROGRAM_ID)
                                    {
                                        remove.push(*key);
                                    }
                                }
                                None => {
                                    remove.push(*key);
                                }
                            }
                        }

                        if sender.send(remove).is_err() {
                            error!("Failed send marked keys");
                        }
                    }
                });
            }
        }
    }
}

#[derive(Clone)]
struct Purge<E: EntityTrait + ColumnAccess, P: DbConn + Send + Sync + 'static> {
    pool: DbPool<P>,
    _e_type: std::marker::PhantomData<E>,
}

impl<E: EntityTrait + ColumnAccess, P: DbConn + Send + Sync + 'static> Purge<E, P> {
    fn new(pool: DbPool<P>) -> Self {
        Self {
            pool,
            _e_type: std::marker::PhantomData,
        }
    }

    async fn purge(&self, keys: Vec<Pubkey>) {
        let keys = keys
            .iter()
            .map(|k| k.to_bytes().to_vec())
            .collect::<Vec<Vec<u8>>>();

        let conn = self.pool.connection();

        let delete_res = E::delete_many()
            .filter(E::column().is_in(keys.iter().map(|a| a.as_slice())))
            .exec(&conn)
            .await;

        if let Ok(res) = delete_res {
            debug!("Successfully purged: {:?}", res.rows_affected);
        } else {
            let keys = keys
                .iter()
                .map(|a| bs58::encode(a).into_string())
                .collect::<Vec<String>>();
            error!("Failed to purge: {:?}", keys);
        }
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

pub async fn start_ta_purge<P: DbConn + Send + Sync + 'static>(
    args: Args,
    db_pool: DbPool<P>,
    rpc: Rpc,
) -> Result<()> {
    let worker_count = args.config.workers as usize;

    let start = tokio::time::Instant::now();

    let (paginate_sender, paginate_receiver) = unbounded_channel::<Vec<Pubkey>>();
    let (mark_sender, mut mark_receiver) = unbounded_channel::<Vec<Pubkey>>();

    let paginate_db_pool = db_pool.clone();
    let paginate_handle = tokio::spawn(async move {
        let paginate = Paginate::<token_accounts::Entity, P>::new(
            paginate_db_pool,
            args.config.batch_size,
            paginate_sender,
        );

        paginate.page().await;
    });

    let mark_handle = tokio::spawn(async move {
        let mut mark = MarkDeletion::new(rpc, paginate_receiver, mark_sender);

        mark.mark().await;
    });

    let purge_handle = tokio::spawn(async move {
        let mut tasks = JoinSet::new();
        let purge = Purge::<token_accounts::Entity, P>::new(db_pool);

        while let Some(addresses) = mark_receiver.recv().await {
            if tasks.len() >= worker_count {
                tasks.join_next().await;
            }

            let purge = purge.clone();

            tasks.spawn(async move {
                purge.purge(addresses).await;
            });
        }

        while tasks.join_next().await.is_some() {}
    });

    futures::future::join3(paginate_handle, mark_handle, purge_handle).await;

    debug!("Purge took: {:?}", start.elapsed());

    Ok(())
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
