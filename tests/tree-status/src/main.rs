use {
    anchor_client::anchor_lang::AnchorDeserialize,
    anyhow::Context,
    async_recursion::async_recursion,
    clap::{arg, Parser, Subcommand},
    digital_asset_types::dao::cl_items,
    sea_orm::{
        sea_query::Expr, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, DbErr,
        EntityTrait, FromQueryResult, QueryFilter, QuerySelect, QueryTrait, SqlxPostgresConnector,
        Statement,
    },
    // plerkle_serialization::serializer::seralize_encoded_transaction_with_status,
    // solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config,
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey},
    solana_sdk::{signature::Signature, transaction::VersionedTransaction},
    solana_transaction_status::{
        option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
        UiTransactionEncoding, UiTransactionStatusMeta,
    },
    // solana_sdk::signature::Signature,
    // solana_transaction_status::UiTransactionEncoding,
    spl_account_compression::state::{
        merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
    },
    spl_account_compression::{AccountCompressionEvent, ChangeLogEvent},
    sqlx::postgres::{PgConnectOptions, PgPoolOptions},

    std::{cmp::Ordering, str::FromStr},
    thiserror::Error,
    tokio_stream::StreamExt,
    txn_forwarder::utils::Siggrabbenheimer,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransactionParsingError {
    #[error("Meta parsing error: {0}")]
    MetaError(String),
    #[error("Transaction decoding error: {0}")]
    DecodingError(String),
}
#[derive(Debug, FromQueryResult, Clone)]
struct MaxSeqItem {
    max_seq: i64,
    cnt_seq: i64,
}

#[allow(dead_code)]
#[derive(Debug, FromQueryResult, Clone)]
struct MissingSeq {
    missing_seq: i64,
}

#[derive(Parser)]
#[command(next_line_help = true, author, version, about)]
struct Args {
    #[arg(short, long)]
    rpc_url: String,

    #[arg(long, short, default_value_t = 3)]
    max_retries: u8,

    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand, Clone)]
enum Action {
    /// Checks a single merkle tree to check if it;s fully indexed
    CheckTree {
        #[arg(short, long)]
        pg_url: String,

        #[arg(short, long, help = "Takes a single pubkey as a parameter to check")]
        tree: String,
    },
    /// Checks a list of merkle trees to check if they're fully indexed
    CheckTrees {
        #[arg(short, long)]
        pg_url: String,

        #[arg(
            short,
            long,
            help = "Takes a path to a file with pubkeys as a parameter to check"
        )]
        file: String,
    },
    // Show a tree
    ShowTree {
        #[arg(short, long, help = "Takes a single tree as a parameter to check")]
        tree: String,
    },
    // Shows a list of trees
    ShowTrees {
        #[arg(short, long, help = "Takes a single tree as a parameter to check")]
        file: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Set up RPC interface
    let pubkeys = match args.action.clone() {
        Action::CheckTree { tree, .. } | Action::ShowTree { tree } => vec![tree],
        Action::CheckTrees { file, .. } | Action::ShowTrees { file } => {
            tokio::fs::read_to_string(&file)
                .await
                .with_context(|| format!("failed to read file with keys: {:?}", file))?
                .split('\n')
                .filter_map(|x| {
                    let x = x.trim();
                    (!x.is_empty()).then(|| x.to_string())
                })
                .collect()
        }
    };

    match args.action.clone() {
        Action::CheckTree { pg_url, .. } | Action::CheckTrees { pg_url, .. } => {
            // Set up db connection
            let url = pg_url;
            let options: PgConnectOptions = url.parse().unwrap();

            // Create postgres pool
            let pool = PgPoolOptions::new()
                .min_connections(2)
                .max_connections(10)
                .connect_with(options)
                .await
                .unwrap();

            // Create new postgres connection
            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());

            let client = RpcClient::new(args.rpc_url);
            check_trees(pubkeys, &conn, &client).await?;
        }
        Action::ShowTree { .. } | Action::ShowTrees { .. } => {
            for pubkey in pubkeys {
                println!("showing tree {:?}", pubkey);
                read_tree(pubkey, args.rpc_url.clone(), false, args.max_retries).await;
            }
        }
    }

    Ok(())
}

async fn check_trees(
    pubkeys: Vec<String>,
    conn: &DatabaseConnection,
    client: &RpcClient,
) -> anyhow::Result<()> {
    for pubkey in pubkeys {
        match pubkey.parse() {
            Ok(pubkey) => {
                let seq = get_tree_latest_seq(pubkey, client).await;
                //println!("seq for pubkey {:?}: {:?}", pubkey, seq);
                if seq.is_err() {
                    eprintln!(
                        "[{:?}] tree is missing from chain or error occurred: {:?}",
                        pubkey, seq
                    );
                    continue;
                }

                let seq = seq.unwrap();

                let fetch_seq = get_tree_max_seq(&pubkey.to_bytes(), conn).await;
                if fetch_seq.is_err() {
                    eprintln!(
                        "[{:?}] couldn't query tree from index: {:?}",
                        pubkey, fetch_seq
                    );
                    continue;
                }
                match fetch_seq.unwrap() {
                    Some(indexed_seq) => {
                        let mut indexing_successful = false;
                        // Check tip
                        match indexed_seq.max_seq.cmp(&seq.try_into().unwrap()) {
                            Ordering::Less => {
                                eprintln!(
                                    "[{:?}] tree not fully indexed: {:?} < {:?}",
                                    pubkey, indexed_seq.max_seq, seq
                                );
                            }
                            Ordering::Equal => {
                                indexing_successful = true;
                            }
                            Ordering::Greater => {
                                eprintln!(
                                    "[{:?}] indexer error: {:?} > {:?}",
                                    pubkey, indexed_seq.max_seq, seq
                                );
                            }
                        }

                        // Check completeness
                        if indexed_seq.max_seq != indexed_seq.cnt_seq {
                            eprintln!(
                                "[{:?}] tree has gaps {:?} != {:?}",
                                pubkey, indexed_seq.max_seq, indexed_seq.cnt_seq
                            );
                            indexing_successful = false;
                        }

                        if indexing_successful {
                            println!("[{:?}] indexing is complete, seq={:?}", pubkey, seq)
                        } else {
                            eprintln!(
                                "[{:?}] indexing is failed, seq={:?} max_seq={:?}",
                                pubkey, seq, indexed_seq
                            );
                            let ret =
                                get_missing_seq(&pubkey.to_bytes(), seq.try_into().unwrap(), conn)
                                    .await;
                            if ret.is_err() {
                                eprintln!("[{:?}] failed to query missing seq: {:?}", pubkey, ret);
                            } else {
                                let ret = ret.unwrap();
                                eprintln!("[{:?}] missing seq: {:?}", pubkey, ret);
                            }
                        }
                    }
                    None => {
                        eprintln!("[{:?}] tree  missing from index", pubkey)
                    }
                }
            }
            Err(error) => {
                eprintln!("failed to parse pubkey {:?}, reason: {:?}", pubkey, error);
            }
        }
    }

    Ok(())
}

async fn get_missing_seq(
    tree: &[u8],
    max_seq: i64,
    conn: &DatabaseConnection,
) -> Result<Vec<MissingSeq>, DbErr> {
    let query = Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT s.seq AS missing_seq FROM generate_series(1::bigint,{}::bigint) s(seq) WHERE NOT EXISTS (SELECT 1 FROM cl_items WHERE seq = s.seq AND tree='\\x{}')", max_seq, hex::encode(tree))
        );

    let missing_sequence_numbers: Vec<MissingSeq> = conn.query_all(query).await.map(|qr| {
        qr.iter()
            .map(|q| MissingSeq::from_query_result(q, "").unwrap())
            .collect()
    })?;

    Ok(missing_sequence_numbers)
}

async fn get_tree_max_seq(
    tree: &[u8],
    conn: &DatabaseConnection,
) -> Result<Option<MaxSeqItem>, DbErr> {
    let query = cl_items::Entity::find()
        .select_only()
        .filter(cl_items::Column::Tree.eq(tree))
        .column_as(Expr::col(cl_items::Column::Seq).max(), "max_seq")
        .column_as(Expr::cust("count(distinct seq)"), "cnt_seq")
        .build(DbBackend::Postgres);

    let res = MaxSeqItem::find_by_statement(query).one(conn).await?;
    Ok(res)
}

async fn get_tree_latest_seq(address: Pubkey, client: &RpcClient) -> anyhow::Result<u64> {
    // get account info
    let account_info = client
        .get_account_with_commitment(&address, CommitmentConfig::confirmed())
        .await?;

    let mut account = account_info
        .value
        .ok_or_else(|| anyhow::anyhow!("No account found"))?;

    let (header_bytes, rest) = account
        .data
        .split_at_mut(CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1);
    let header: ConcurrentMerkleTreeHeader =
        ConcurrentMerkleTreeHeader::try_from_slice(header_bytes)?;

    // let auth = Pubkey::find_program_address(&[address.as_ref()], &mpl_bubblegum::id()).0;

    let merkle_tree_size = merkle_tree_get_size(&header)?;
    let (tree_bytes, _canopy_bytes) = rest.split_at_mut(merkle_tree_size);

    let seq_bytes = tree_bytes[0..8].try_into().context("Error parsing bytes")?;
    Ok(u64::from_le_bytes(seq_bytes))
}

// Fetches all the transactions referencing a specific trees
pub async fn read_tree(address: String, client_url: String, failed: bool, max_retries: u8) {
    let client1 = RpcClient::new(client_url.clone());
    let pub_addr = Pubkey::from_str(address.as_str()).unwrap();
    // This takes a param failed but it excludes all failed TXs
    let mut sig = Siggrabbenheimer::new(client1, pub_addr, failed);
    let mut tasks = Vec::new();
    while let Some(s) = sig.next().await {
        let client_url = client_url.clone();
        tasks.push(tokio::spawn(async move {
            let client2 = RpcClient::new(client_url.clone());
            process_txn(&s, &client2, max_retries).await;
        }))
    }
    for task in tasks {
        task.await.unwrap();
    }
}

// Process and individual transaction, fetching it and reading out the sequence numbers
#[async_recursion]
pub async fn process_txn(sig_str: &str, client: &RpcClient, retries: u8) {
    let sig = Signature::from_str(sig_str).unwrap();
    let tx = client
        .get_transaction_with_config(
            &sig,
            solana_client::rpc_config::RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Base64),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            },
        )
        .await;

    match tx {
        Ok(txn) => {
            let seq_numbers = parse_txn_sequence(&txn).await;
            if let Ok(arr) = seq_numbers {
                for seq in arr {
                    println!("{} {}", seq, sig);
                }
            }
        }
        Err(e) => {
            if retries > 0 {
                eprintln!("Retrying transaction {} retry no {}: {}", sig, retries, e);
                process_txn(sig_str, client, retries - 1).await;
            } else {
                eprintln!("Could not load transaction {}: {}", sig, e);
            }
        }
    }
}

// Parse the trasnaction data
pub async fn parse_txn_sequence(
    txn: &EncodedConfirmedTransactionWithStatusMeta,
) -> Result<Vec<u64>, TransactionParsingError> {
    let mut seq_updates = vec![];

    // Get `UiTransaction` out of `EncodedTransactionWithStatusMeta`.
    let meta: UiTransactionStatusMeta =
        txn.transaction
            .meta
            .clone()
            .ok_or(TransactionParsingError::MetaError(String::from(
                "couldn't load meta",
            )))?;
    let transaction: VersionedTransaction =
        txn.transaction
            .transaction
            .decode()
            .ok_or(TransactionParsingError::DecodingError(String::from(
                "Couldn't parse transction",
            )))?;

    let mut account_keys = transaction.message.static_account_keys().to_vec();
    // Add the account lookup stuff
    if let OptionSerializer::Some(loaded_addresses) = meta.loaded_addresses {
        loaded_addresses.writable.iter().for_each(|pkey| {
            account_keys.push(Pubkey::from_str(pkey).unwrap());
        });
        loaded_addresses.readonly.iter().for_each(|pkey| {
            account_keys.push(Pubkey::from_str(pkey).unwrap());
        });
    }

    // See https://github.com/ngundotra/spl-ac-seq-parse/blob/main/src/main.rs
    if let OptionSerializer::Some(inner_instructions_vec) = meta.inner_instructions.as_ref() {
        for inner_ixs in inner_instructions_vec.iter() {
            for (_, inner_ix) in inner_ixs.instructions.iter().enumerate() {
                if let solana_transaction_status::UiInstruction::Compiled(instr) = inner_ix {
                    if let Some(program) = account_keys.get(instr.program_id_index as usize) {
                        if program.to_string() == spl_noop::id().to_string() {
                            let data = bs58::decode(&instr.data).into_vec().map_err(|_| {
                                TransactionParsingError::DecodingError(String::from(
                                    "error base58ing",
                                ))
                            })?;
                            if let Ok(AccountCompressionEvent::ChangeLog(cl_data)) =
                                &AccountCompressionEvent::try_from_slice(&data)
                            {
                                let ChangeLogEvent::V1(cl_data) = cl_data;
                                seq_updates.push(cl_data.seq);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(seq_updates)
}
