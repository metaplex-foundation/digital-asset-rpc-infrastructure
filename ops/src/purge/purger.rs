use anyhow::Result;
use borsh::BorshDeserialize;
use clap::Parser;
use das_core::{DatabasePool, Rpc};
use digital_asset_types::dao::{asset, token_accounts, tokens};
use log::{debug, error};
use sea_orm::{
    sea_query::Expr, ColumnTrait, EntityTrait, FromQueryResult, PaginatorTrait, QueryFilter,
    QuerySelect,
};
use solana_sdk::{bs58, pubkey};
use std::marker::{Send, Sync};

use solana_sdk::pubkey::Pubkey;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::{JoinHandle, JoinSet};

pub const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

pub const TOKEN_2022_PROGRAM_ID: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

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

pub trait PubkeyResult: FromQueryResult + Send + Sync {
    fn pubkey(&self) -> Vec<u8>;
}

#[derive(FromQueryResult)]
struct TokenAccountResult {
    pubkey: Vec<u8>,
}

#[derive(FromQueryResult)]
struct MintResult {
    mint: Vec<u8>,
}

impl PubkeyResult for TokenAccountResult {
    fn pubkey(&self) -> Vec<u8> {
        self.pubkey.clone()
    }
}

impl PubkeyResult for MintResult {
    fn pubkey(&self) -> Vec<u8> {
        self.mint.clone()
    }
}

struct Paginate<E: EntityTrait + ColumnAccess, P: DatabasePool, R: PubkeyResult> {
    pool: Option<P>,
    batch_size: Option<u64>,
    sender: Option<UnboundedSender<Vec<Pubkey>>>,
    _e_type: std::marker::PhantomData<E>,
    _r_type: std::marker::PhantomData<R>,
}

impl<E: EntityTrait + ColumnAccess, P: DatabasePool, R: PubkeyResult> Paginate<E, P, R> {
    pub const DEFAULT_DB_BATCH_SIZE: u64 = 100;

    fn build() -> Self {
        Self {
            pool: None,
            batch_size: None,
            sender: None,
            _e_type: std::marker::PhantomData,
            _r_type: std::marker::PhantomData,
        }
    }

    fn pool(mut self, pool: P) -> Self {
        self.pool = Some(pool);
        self
    }

    fn batch_size(mut self, batch_size: u64) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    fn sender(mut self, sender: UnboundedSender<Vec<Pubkey>>) -> Self {
        self.sender = Some(sender);
        self
    }

    fn start(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let pool = self.pool.expect("Pool not set");
            let sender = self.sender.expect("Sender not set");
            let batch_size = self.batch_size.unwrap_or(Self::DEFAULT_DB_BATCH_SIZE);
            let conn = pool.connection();

            let mut paginator = E::find()
                .column(E::column())
                .into_model::<R>()
                .paginate(&conn, batch_size);

            while let Ok(Some(records)) = paginator.fetch_and_next().await {
                let keys = records
                    .iter()
                    .filter_map(|row| Pubkey::try_from_slice(&row.pubkey()).ok())
                    .collect::<Vec<Pubkey>>();

                if sender.send(keys).is_err() {
                    error!("Failed to send keys");
                }
            }
        })
    }
}

struct MarkDeletion {
    rpc: Option<Rpc>,
    receiver: Option<UnboundedReceiver<Vec<Pubkey>>>,
    sender: Option<UnboundedSender<Vec<Pubkey>>>,
    concurrency: Option<usize>,
}

impl MarkDeletion {
    pub const DEFAULT_CONCURRENCY: usize = 10;

    pub const MAX_GET_MULTIPLE_ACCOUNTS: usize = 100;

    fn build() -> Self {
        Self {
            rpc: None,
            receiver: None,
            sender: None,
            concurrency: None,
        }
    }

    fn rpc(mut self, rpc: Rpc) -> Self {
        self.rpc = Some(rpc);
        self
    }

    fn receiver(mut self, receiver: UnboundedReceiver<Vec<Pubkey>>) -> Self {
        self.receiver = Some(receiver);
        self
    }

    fn sender(mut self, sender: UnboundedSender<Vec<Pubkey>>) -> Self {
        self.sender = Some(sender);
        self
    }

    fn concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    fn start(self) -> JoinHandle<()> {
        let rpc = self.rpc.expect("Rpc not set");
        let mut receiver = self.receiver.expect("Receiver not set");
        let sender = self.sender.expect("Sender not set");
        let concurrency = self.concurrency.unwrap_or(Self::DEFAULT_CONCURRENCY);

        tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            while let Some(keys) = receiver.recv().await {
                let rpc = rpc.clone();
                let sender = sender.clone();
                if tasks.len() >= concurrency {
                    tasks.join_next().await;
                }

                tasks.spawn(async move {
                    for chunk in keys.chunks(Self::MAX_GET_MULTIPLE_ACCOUNTS) {
                        let chunk = chunk.to_vec();
                        debug!("chunk len: {}", chunk.len());

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
                            if remove.is_empty() {
                                continue;
                            }
                            debug!("entries to purge: {:?}", remove.len());
                            if sender.send(remove).is_err() {
                                error!("Failed to send marked keys");
                            }
                        }
                    }
                });
            }
            while tasks.join_next().await.is_some() {}
        })
    }
}

trait Purge {
    async fn purge(&self, keys: Vec<Pubkey>);
}

#[derive(Clone)]
struct TokenAccountPurge<P: DatabasePool> {
    pool: P,
}

impl<P: DatabasePool> TokenAccountPurge<P> {
    pub fn new(pool: P) -> Self {
        Self { pool }
    }
}

impl<P: DatabasePool> Purge for TokenAccountPurge<P> {
    async fn purge(&self, keys: Vec<Pubkey>) {
        let keys = keys
            .iter()
            .map(|k| k.to_bytes().to_vec())
            .collect::<Vec<Vec<u8>>>();

        let conn = self.pool.connection();

        let delete_res = token_accounts::Entity::delete_many()
            .filter(token_accounts::Column::Pubkey.is_in(keys.iter().map(|a| a.as_slice())))
            .exec(&conn)
            .await;

        let keys = keys
            .iter()
            .map(|a| bs58::encode(a).into_string())
            .collect::<Vec<String>>();

        if let Ok(res) = delete_res {
            debug!(
                "Successfully purged token_accounts: {:?}",
                res.rows_affected
            );
        } else {
            error!("Failed to purge token_accounts: {:?}", keys);
        }
    }
}

#[derive(Clone)]
struct MintPurge<P: DatabasePool> {
    pool: P,
}

impl<P: DatabasePool> MintPurge<P> {
    pub fn new(pool: P) -> Self {
        Self { pool }
    }
}

impl<P: DatabasePool> Purge for MintPurge<P> {
    async fn purge(&self, keys: Vec<Pubkey>) {
        let keys = keys
            .iter()
            .map(|k| k.to_bytes().to_vec())
            .collect::<Vec<Vec<u8>>>();

        let conn = self.pool.connection();

        let (delete_res, update_res) = tokio::join!(
            tokens::Entity::delete_many()
                .filter(tokens::Column::Mint.is_in(keys.iter().map(|a| a.as_slice())))
                .exec(&conn),
            asset::Entity::update_many()
                .col_expr(asset::Column::Burnt, Expr::value(true))
                .filter(asset::Column::Burnt.eq(false))
                .filter(asset::Column::Id.is_in(keys.clone()))
                .exec(&conn)
        );

        let keys = keys
            .iter()
            .map(|a| bs58::encode(a).into_string())
            .collect::<Vec<String>>();

        if let Ok(res) = delete_res {
            debug!("Successfully purged tokens: {:?}", res.rows_affected);
        } else {
            error!("Failed to purge tokens: {:?}", keys);
        }

        if let Ok(res) = update_res {
            debug!(
                "Successfully marked assets as burnt: {:?}",
                res.rows_affected
            );
        } else {
            error!("Failed to update assets: {:?}", keys);
        }
    }
}

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// The number of worker threads
    #[arg(long, env, default_value = "25")]
    pub purge_worker_count: u64,
    /// The number of db entries to process in a single batch
    #[arg(long, env, default_value = "100")]
    pub mark_deletion_worker_count: u64,
    /// The number of db entries to process in a single batch
    #[arg(long, env, default_value = "100")]
    pub batch_size: u64,
}

pub async fn start_ta_purge<P: DatabasePool>(args: Args, db: P, rpc: Rpc) -> Result<()> {
    let start = tokio::time::Instant::now();

    let (paginate_sender, paginate_receiver) = unbounded_channel::<Vec<Pubkey>>();
    let (mark_sender, mut mark_receiver) = unbounded_channel::<Vec<Pubkey>>();

    let paginate_db = db.clone();

    let paginate_handle = Paginate::<token_accounts::Entity, P, TokenAccountResult>::build()
        .pool(paginate_db)
        .batch_size(args.batch_size)
        .sender(paginate_sender)
        .start();

    let mark_handle = MarkDeletion::build()
        .rpc(rpc)
        .receiver(paginate_receiver)
        .sender(mark_sender)
        .concurrency(args.mark_deletion_worker_count as usize)
        .start();

    let purge_worker_count = args.purge_worker_count as usize;

    let purge_handle = tokio::spawn(async move {
        let mut tasks = JoinSet::new();
        let purge = TokenAccountPurge::new(db);

        while let Some(addresses) = mark_receiver.recv().await {
            if tasks.len() >= purge_worker_count {
                tasks.join_next().await;
            }

            let purge = purge.clone();

            tasks.spawn(async move {
                purge.purge(addresses).await;
            });
        }

        while tasks.join_next().await.is_some() {}
    });

    let _ = futures::future::join3(paginate_handle, mark_handle, purge_handle).await;

    debug!("Purge took: {:?}", start.elapsed());

    Ok(())
}

pub async fn start_mint_purge<P: DatabasePool>(args: Args, db: P, rpc: Rpc) -> Result<()> {
    let start = tokio::time::Instant::now();

    let (paginate_sender, paginate_receiver) = unbounded_channel::<Vec<Pubkey>>();
    let (mark_sender, mut mark_receiver) = unbounded_channel::<Vec<Pubkey>>();

    let paginate_db = db.clone();

    let paginate_handle = Paginate::<tokens::Entity, P, MintResult>::build()
        .pool(paginate_db)
        .batch_size(args.batch_size)
        .sender(paginate_sender)
        .start();

    let mark_handle = MarkDeletion::build()
        .rpc(rpc)
        .receiver(paginate_receiver)
        .sender(mark_sender)
        .concurrency(args.mark_deletion_worker_count as usize)
        .start();

    let purge_worker_count = args.purge_worker_count as usize;

    let purge_handle = tokio::spawn(async move {
        let mut tasks = JoinSet::new();
        let purge = MintPurge::new(db);

        while let Some(addresses) = mark_receiver.recv().await {
            if tasks.len() >= purge_worker_count {
                tasks.join_next().await;
            }

            let purge = purge.clone();

            tasks.spawn(async move {
                purge.purge(addresses).await;
            });
        }

        while tasks.join_next().await.is_some() {}
    });

    let _ = futures::future::join3(paginate_handle, mark_handle, purge_handle).await;

    debug!("Purge took: {:?}", start.elapsed());

    Ok(())
}
