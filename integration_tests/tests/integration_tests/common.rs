use std::path::Path;

use std::str::FromStr;

use das_api::api::DasApi;

use das_api::config::Config;

use migration::sea_orm::{
    ConnectionTrait, DatabaseConnection, ExecResult, SqlxPostgresConnector, Statement,
};
use migration::{Migrator, MigratorTrait};
use mpl_token_metadata::accounts::Metadata;

use nft_ingester::config::{self, rand_string};
use nft_ingester::program_transformers::ProgramTransformer;
use nft_ingester::tasks::TaskManager;
use once_cell::sync::Lazy;
use plerkle_serialization::root_as_account_info;
use plerkle_serialization::root_as_transaction_info;
use plerkle_serialization::serializer::serialize_account;
use plerkle_serialization::solana_geyser_plugin_interface_shims::ReplicaAccountInfoV2;

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;

use futures_util::StreamExt as FuturesStreamExt;
use futures_util::TryStreamExt;
use tokio_stream::{self as stream};

use log::{error, info};
use plerkle_serialization::serializer::seralize_encoded_transaction_with_status;
// use rand::seq::SliceRandom;
use serde::de::DeserializeOwned;
use solana_account_decoder::{UiAccount, UiAccountEncoding};
use solana_client::{
    client_error::ClientError,
    client_error::Result as RpcClientResult,
    rpc_config::{RpcAccountInfoConfig, RpcTransactionConfig},
    rpc_request::RpcRequest,
    rpc_response::{Response as RpcResponse, RpcTokenAccountBalance},
};
use solana_sdk::{
    account::Account,
    commitment_config::{CommitmentConfig, CommitmentLevel},
};
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding};
use std::{fmt, time::Duration};

use std::path::PathBuf;

use tokio::time::sleep;

pub const DEFAULT_SLOT: u64 = 1;

pub struct TestSetup {
    pub name: String,
    pub client: RpcClient,
    pub db: Arc<DatabaseConnection>,
    pub transformer: ProgramTransformer,
    pub das_api: DasApi,
}

impl TestSetup {
    pub async fn new(name: String) -> Self {
        Self::new_with_options(name, TestSetupOptions::default()).await
    }

    pub async fn new_with_options(name: String, opts: TestSetupOptions) -> Self {
        let database_test_url = std::env::var("DATABASE_TEST_URL").unwrap();
        let mut database_config = config::DatabaseConfig::new();
        database_config.insert("database_url".to_string(), database_test_url.clone().into());

        if !(database_test_url.contains("localhost") || database_test_url.contains("127.0.0.1")) {
            panic!("Tests can only be run on a local database");
        }

        let pool = setup_pg_pool(database_test_url.clone()).await;
        let db = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
        let transformer = load_ingest_program_transformer(pool.clone()).await;

        let rpc_url = match opts.network.unwrap_or_default() {
            Network::Mainnet => std::env::var("MAINNET_RPC_URL").unwrap(),
            Network::Devnet => std::env::var("DEVNET_RPC_URL").unwrap(),
        };
        let client = RpcClient::new(rpc_url.to_string());

        let das_api_config: Config = das_api::config::Config {
            database_url: database_test_url.to_string(),
            ..Default::default()
        };
        let das_api = das_api::api::DasApi::from_config(das_api_config)
            .await
            .unwrap();

        TestSetup {
            name,
            client,
            db: Arc::new(db),
            transformer,
            das_api,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct TestSetupOptions {
    pub network: Option<Network>,
}

pub async fn setup_pg_pool(database_url: String) -> PgPool {
    let options: PgConnectOptions = database_url.parse().unwrap();
    PgPoolOptions::new()
        .min_connections(1)
        .connect_with(options)
        .await
        .unwrap()
}

pub async fn truncate_table(
    db: Arc<DatabaseConnection>,
    table: String,
) -> Result<migration::sea_orm::ExecResult, migration::DbErr> {
    let raw_sql = format!("TRUNCATE TABLE {} CASCADE", table);
    db.execute(Statement::from_string(db.get_database_backend(), raw_sql))
        .await
}

static INIT: Lazy<Mutex<Option<()>>> = Lazy::new(|| Mutex::new(None));

pub async fn apply_migrations_and_delete_data(db: Arc<DatabaseConnection>) {
    let mut init = INIT.lock().await;
    if init.is_none() {
        std::env::set_var("INIT_FILE_PATH", "../init.sql");
        Migrator::fresh(&db).await.unwrap();
        *init = Some(());
        // Mutex will dropped once it goes out of scope.
        return;
    }

    let tables: Vec<String> = db
        .query_all(Statement::from_string(db.get_database_backend(), "SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname != 'pg_catalog' AND schemaname != 'information_schema' AND tablename != 'seaql_migrations'".to_string()))
        .await
        .unwrap().into_iter()
        .map(|row| row.try_get("", "tablename").unwrap()).collect::<Vec<String>>();

    let max_concurrency = 10;

    stream::iter(tables.into_iter())
        .map(|table| truncate_table(db.clone(), table.clone()))
        .buffer_unordered(max_concurrency)
        .try_collect::<Vec<ExecResult>>()
        .await
        .unwrap();
}

async fn load_ingest_program_transformer(pool: sqlx::Pool<sqlx::Postgres>) -> ProgramTransformer {
    // HACK: We don't really use this background task handler but we need it to create the sender
    let mut background_task_manager = TaskManager::new(rand_string(), pool.clone(), vec![]);
    background_task_manager.start_listener(true);
    let bg_task_sender = background_task_manager.get_sender().unwrap();
    ProgramTransformer::new(pool, bg_task_sender, false)
}

pub async fn get_transaction(
    client: &RpcClient,
    signature: Signature,
    max_retries: u8,
) -> Result<EncodedConfirmedTransactionWithStatusMeta, ClientError> {
    let mut retries = 0;
    let mut delay = Duration::from_millis(500);

    const CONFIG: RpcTransactionConfig = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Base64),
        commitment: Some(CommitmentConfig {
            commitment: CommitmentLevel::Confirmed,
        }),
        max_supported_transaction_version: Some(0),
    };

    loop {
        let response = client
            .send(
                RpcRequest::GetTransaction,
                serde_json::json!([signature.to_string(), CONFIG,]),
            )
            .await;

        if let Err(error) = response {
            if retries < max_retries {
                error!("failed to get transaction {:?}: {:?}", signature, error);
                sleep(delay).await;
                delay *= 2;
                retries += 1;
                continue;
            } else {
                return Err(error);
            }
        }
        return response;
    }
}

pub async fn fetch_and_serialize_transaction(
    client: &RpcClient,
    sig: Signature,
) -> anyhow::Result<Option<Vec<u8>>> {
    let max_retries = 5;
    let tx: EncodedConfirmedTransactionWithStatusMeta =
        get_transaction(client, sig, max_retries).await?;

    // Ignore if tx failed or meta is missed
    let meta = tx.transaction.meta.as_ref();
    if meta.map(|meta| meta.status.is_err()).unwrap_or(true) {
        info!("Ignoring failed transaction: {}", sig);
        return Ok(None);
    }
    let fbb = flatbuffers::FlatBufferBuilder::new();
    let fbb = seralize_encoded_transaction_with_status(fbb, tx)?;
    let serialized = fbb.finished_data();

    Ok(Some(serialized.to_vec()))
}

// Util functions for accounts
pub async fn rpc_tx_with_retries<T, E>(
    client: &RpcClient,
    request: RpcRequest,
    value: serde_json::Value,
    max_retries: u8,
    error_key: E,
) -> RpcClientResult<T>
where
    T: DeserializeOwned,
    E: fmt::Debug,
{
    let mut retries = 0;
    let mut delay = Duration::from_millis(500);
    loop {
        match client.send(request, value.clone()).await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if retries < max_retries {
                    error!("retrying {request} {error_key:?}: {error}");
                    sleep(delay).await;
                    delay *= 2;
                    retries += 1;
                } else {
                    return Err(error);
                }
            }
        }
    }
}

pub async fn fetch_account(
    pubkey: Pubkey,
    client: &RpcClient,
    max_retries: u8,
) -> anyhow::Result<(Account, u64)> {
    const CONFIG: RpcAccountInfoConfig = RpcAccountInfoConfig {
        encoding: Some(UiAccountEncoding::Base64Zstd),
        commitment: Some(CommitmentConfig {
            commitment: CommitmentLevel::Finalized,
        }),
        data_slice: None,
        min_context_slot: None,
    };

    let response: RpcResponse<Option<UiAccount>> = rpc_tx_with_retries(
        client,
        RpcRequest::GetAccountInfo,
        serde_json::json!([pubkey.to_string(), CONFIG]),
        max_retries,
        pubkey,
    )
    .await?;

    let account: Account = response
        .value
        .ok_or_else(|| anyhow::anyhow!("failed to get account {pubkey}"))?
        .decode()
        .ok_or_else(|| anyhow::anyhow!("failed to parse account {pubkey}"))?;

    Ok((account, response.context.slot))
}

pub async fn fetch_and_serialize_account(
    client: &RpcClient,
    pubkey: Pubkey,
    slot: Option<u64>,
) -> anyhow::Result<Vec<u8>> {
    let max_retries = 5;

    let fetch_result = fetch_account(pubkey, client, max_retries).await;

    let (account, actual_slot) = match fetch_result {
        Ok((account, actual_slot)) => (account, actual_slot),
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to fetch account: {:?}", e));
        }
    };

    let fbb = flatbuffers::FlatBufferBuilder::new();
    let account_info = ReplicaAccountInfoV2 {
        pubkey: &pubkey.to_bytes(),
        lamports: account.lamports,
        owner: &account.owner.to_bytes(),
        executable: account.executable,
        rent_epoch: account.rent_epoch,
        data: &account.data,
        write_version: 0,
        txn_signature: None,
    };
    let is_startup = false;

    let fbb = serialize_account(
        fbb,
        &account_info,
        match slot {
            Some(slot) => slot,
            None => actual_slot,
        },
        is_startup,
    );
    Ok(fbb.finished_data().to_vec())
}

pub async fn get_token_largest_account(client: &RpcClient, mint: Pubkey) -> anyhow::Result<Pubkey> {
    let response: RpcResponse<Vec<RpcTokenAccountBalance>> = rpc_tx_with_retries(
        client,
        RpcRequest::Custom {
            method: "getTokenLargestAccounts",
        },
        serde_json::json!([mint.to_string(),]),
        5,
        mint,
    )
    .await?;

    match response.value.first() {
        Some(account) => {
            let pubkey = Pubkey::from_str(&account.address);
            match pubkey {
                Ok(pubkey) => Ok(pubkey),
                Err(e) => anyhow::bail!("failed to parse pubkey: {:?}", e),
            }
        }
        None => anyhow::bail!("no accounts for mint {mint}: burned nft?"),
    }
}

pub async fn index_account_bytes(setup: &TestSetup, account_bytes: Vec<u8>) {
    let account = root_as_account_info(&account_bytes).unwrap();

    setup
        .transformer
        .handle_account_update(account)
        .await
        .unwrap();
}

pub async fn cached_fetch_account(
    setup: &TestSetup,
    account: Pubkey,
    slot: Option<u64>,
) -> Vec<u8> {
    cached_fetch_account_with_error_handling(setup, account, slot)
        .await
        .unwrap()
}

fn get_relative_project_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

async fn cached_fetch_account_with_error_handling(
    setup: &TestSetup,
    account: Pubkey,
    slot: Option<u64>,
) -> anyhow::Result<Vec<u8>> {
    let dir = get_relative_project_path(&format!("tests/data/accounts/{}", setup.name));

    if !Path::new(&dir).exists() {
        std::fs::create_dir(&dir).unwrap();
    }
    let file_path = dir.join(format!("{}", account));

    if file_path.exists() {
        Ok(std::fs::read(file_path).unwrap())
    } else {
        let account_bytes = fetch_and_serialize_account(&setup.client, account, slot).await?;
        std::fs::write(file_path, &account_bytes).unwrap();
        Ok(account_bytes)
    }
}

async fn cached_fetch_transaction(setup: &TestSetup, sig: Signature) -> Vec<u8> {
    let dir = get_relative_project_path(&format!("tests/data/transactions/{}", setup.name));

    if !Path::new(&dir).exists() {
        std::fs::create_dir(&dir).unwrap();
    }
    let file_path = dir.join(format!("{}", sig));

    if file_path.exists() {
        std::fs::read(file_path).unwrap()
    } else {
        let txn_bytes = fetch_and_serialize_transaction(&setup.client, sig)
            .await
            .unwrap()
            .unwrap();
        std::fs::write(file_path, &txn_bytes).unwrap();
        txn_bytes
    }
}

pub async fn index_transaction(setup: &TestSetup, sig: Signature) {
    let txn_bytes: Vec<u8> = cached_fetch_transaction(setup, sig).await;
    let txn = root_as_transaction_info(&txn_bytes).unwrap();
    setup.transformer.handle_transaction(&txn).await.unwrap();
}

async fn cached_fetch_largest_token_account_id(client: &RpcClient, mint: Pubkey) -> Pubkey {
    let dir = get_relative_project_path(&format!("tests/data/largest_token_account_ids/{}", mint));

    if !Path::new(&dir).exists() {
        std::fs::create_dir(&dir).unwrap();
    }
    let file_path = dir.join(format!("{}", mint));

    if file_path.exists() {
        Pubkey::try_from(std::fs::read(file_path).unwrap()).unwrap()
    } else {
        let token_account = get_token_largest_account(client, mint).await.unwrap();
        std::fs::write(file_path, token_account.to_bytes()).unwrap();
        token_account
    }
}

#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub enum SeedEvent {
    Account(Pubkey),
    Nft(Pubkey),
    TokenMint(Pubkey),
    Signature(Signature),
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Network {
    #[default]
    Mainnet,
    Devnet,
}

#[derive(Clone, Copy, Debug)]
pub enum Order {
    Forward,
    AllPermutations,
}

pub async fn index_seed_events(setup: &TestSetup, events: Vec<&SeedEvent>) {
    for event in events {
        match event {
            SeedEvent::Account(account) => {
                index_account_with_ordered_slot(setup, *account).await;
            }
            SeedEvent::Nft(mint) => {
                index_nft(setup, *mint).await;
            }
            SeedEvent::Signature(sig) => {
                index_transaction(setup, *sig).await;
            }
            SeedEvent::TokenMint(mint) => {
                index_token_mint(setup, *mint).await;
            }
        }
    }
}

#[allow(unused)]
pub fn seed_account(str: &str) -> SeedEvent {
    SeedEvent::Account(Pubkey::from_str(str).unwrap())
}

pub fn seed_nft(str: &str) -> SeedEvent {
    SeedEvent::Nft(Pubkey::from_str(str).unwrap())
}

#[allow(unused)]
pub fn seed_token_mint(str: &str) -> SeedEvent {
    SeedEvent::TokenMint(Pubkey::from_str(str).unwrap())
}

pub fn seed_txn(str: &str) -> SeedEvent {
    SeedEvent::Signature(Signature::from_str(str).unwrap())
}

pub fn seed_txns<I>(strs: I) -> Vec<SeedEvent>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    strs.into_iter().map(|s| seed_txn(s.as_ref())).collect()
}

#[allow(unused)]
pub fn seed_accounts<I>(strs: I) -> Vec<SeedEvent>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    strs.into_iter().map(|s| seed_account(s.as_ref())).collect()
}

pub fn seed_nfts<I>(strs: I) -> Vec<SeedEvent>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    strs.into_iter().map(|s| seed_nft(s.as_ref())).collect()
}

#[allow(unused)]
pub fn seed_token_mints<I>(strs: I) -> Vec<SeedEvent>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    strs.into_iter()
        .map(|s| seed_token_mint(s.as_ref()))
        .collect()
}

pub async fn index_account(setup: &TestSetup, account: Pubkey) {
    // If we used different slots for accounts, then it becomes harder to test updates of related
    // accounts because we need to factor the fact that some updates can be disregarded because
    // they are "stale".
    let slot = Some(DEFAULT_SLOT);
    let account_bytes = cached_fetch_account(setup, account, slot).await;
    index_account_bytes(setup, account_bytes).await;
}

#[derive(Clone, Copy)]
pub struct NftAccounts {
    pub mint: Pubkey,
    pub metadata: Pubkey,
    pub token: Pubkey,
}

pub async fn get_nft_accounts(setup: &TestSetup, mint: Pubkey) -> NftAccounts {
    let metadata_account = Metadata::find_pda(&mint).0;
    let token_account = cached_fetch_largest_token_account_id(&setup.client, mint).await;
    NftAccounts {
        mint,
        metadata: metadata_account,
        token: token_account,
    }
}

async fn index_account_with_ordered_slot(setup: &TestSetup, account: Pubkey) {
    let slot = None;
    let account_bytes = cached_fetch_account(setup, account, slot).await;
    index_account_bytes(setup, account_bytes).await;
}

async fn index_token_mint(setup: &TestSetup, mint: Pubkey) {
    let token_account = cached_fetch_largest_token_account_id(&setup.client, mint).await;
    index_account(setup, mint).await;
    index_account(setup, token_account).await;

    // If we used different slots for accounts, then it becomes harder to test updates of related
    // accounts because we need to factor the fact that some updates can be disregarded because
    // they are "stale".
    let slot = Some(1);
    let metadata_account = Metadata::find_pda(&mint).0;
    match cached_fetch_account_with_error_handling(setup, metadata_account, slot).await {
        Ok(account_bytes) => {
            index_account_bytes(setup, account_bytes).await;
        }
        Err(_) => {
            // If we can't find the metadata account, then we assume that the mint is not an NFT.
        }
    }
}

pub async fn index_nft(setup: &TestSetup, mint: Pubkey) {
    index_nft_accounts(setup, get_nft_accounts(setup, mint).await).await;
}

pub async fn index_nft_accounts(setup: &TestSetup, nft_accounts: NftAccounts) {
    for account in [nft_accounts.mint, nft_accounts.metadata, nft_accounts.token] {
        index_account(setup, account).await;
    }
}

pub fn trim_test_name(name: &str) -> String {
    name.replace("test_", "")
}
