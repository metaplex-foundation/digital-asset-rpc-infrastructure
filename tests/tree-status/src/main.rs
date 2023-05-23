use {
    anchor_client::anchor_lang::AnchorDeserialize,
    anyhow::Context,
    clap::{arg, Parser, Subcommand},
    digital_asset_types::dao::cl_items,
    futures::{
        future::{try_join, try_join_all, BoxFuture, FutureExt, TryFutureExt},
        stream::{self, StreamExt},
    },
    log::{debug, error, info},
    sea_orm::{
        sea_query::{Expr, Value},
        ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, DbErr, EntityTrait,
        FromQueryResult, QueryFilter, QuerySelect, QueryTrait, SqlxPostgresConnector, Statement,
    },
    // plerkle_serialization::serializer::seralize_encoded_transaction_with_status,
    // solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config,
    solana_client::{
        nonblocking::rpc_client::RpcClient, rpc_config::RpcTransactionConfig,
        rpc_request::RpcRequest,
    },
    solana_sdk::{
        commitment_config::{CommitmentConfig, CommitmentLevel},
        pubkey::{ParsePubkeyError, Pubkey},
        signature::Signature,
        transaction::VersionedTransaction,
    },
    solana_transaction_status::{
        option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
        UiTransactionEncoding, UiTransactionStatusMeta,
    },
    // solana_sdk::signature::Signature,
    // solana_transaction_status::UiTransactionEncoding,
    spl_account_compression::{
        state::{
            merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
        },
        AccountCompressionEvent, ChangeLogEvent,
    },
    sqlx::postgres::{PgConnectOptions, PgPoolOptions},
    std::{
        cmp,
        collections::HashMap,
        env,
        num::NonZeroUsize,
        pin::Pin,
        str::FromStr,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    },
    tokio::{
        fs::OpenOptions,
        io::{stdout, AsyncWrite, AsyncWriteExt},
        sync::{mpsc, Mutex},
    },
    txn_forwarder::{find_signatures, read_lines, rpc_send_with_retries},
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("failed to load Transaction Meta")]
    TransactionMeta,
    #[error("failed to decode Transaction")]
    Transaction,
    #[error("failed to decode instruction data: {0}")]
    Instruction(#[from] bs58::decode::Error),
    #[error("failed to parse pubkey: {0}")]
    Pubkey(#[from] ParsePubkeyError),
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

#[derive(Debug, FromQueryResult)]
struct AssetMaxSeq {
    leaf_idx: i64,
    seq: i64,
}

#[derive(Debug)]
struct LeafNode {
    leaf: Vec<u8>,
    index: i64,
}

type MaybeLeafNode = Option<LeafNode>;

#[derive(Parser)]
#[command(next_line_help = true, author, version, about)]
struct Args {
    /// Solana RPC endpoint.
    #[arg(long, short, alias = "rpc-url")]
    rpc: String,

    /// Number of concurrent requests for fetching transactions.
    #[arg(long, short, default_value_t = 25)]
    concurrency: usize,

    /// Maximum number of retries for transaction fetching.
    #[arg(long, short, default_value_t = 3)]
    max_retries: u8,

    #[command(subcommand)]
    action: Action,
}

impl Args {
    async fn get_pg_conn(&self) -> anyhow::Result<DatabaseConnection> {
        match &self.action {
            Action::CheckTree { pg_url, .. }
            | Action::CheckTrees { pg_url, .. }
            | Action::CheckTreeLeafs { pg_url, .. }
            | Action::CheckTreesLeafs { pg_url, .. } => {
                let options: PgConnectOptions = pg_url.parse().unwrap();

                // Create postgres pool
                let pool = PgPoolOptions::new()
                    .min_connections(2)
                    .max_connections(10)
                    .connect_with(options)
                    .await?;

                // Create new postgres connection
                Ok(SqlxPostgresConnector::from_sqlx_postgres_pool(pool))
            }
            Action::ShowTree { .. } | Action::ShowTrees { .. } => {
                anyhow::bail!("show-tree and show-tress do not have connection to database")
            }
        }
    }
}

#[derive(Subcommand, Clone)]
enum Action {
    /// Checks a single merkle tree to check if it's fully indexed
    CheckTree {
        #[arg(short, long)]
        pg_url: String,
        #[arg(short, long, help = "Tree pubkey")]
        tree: String,
    },
    /// Checks a list of merkle trees to check if they're fully indexed
    CheckTrees {
        #[arg(short, long)]
        pg_url: String,
        #[arg(short, long, help = "Path to file with trees pubkeys")]
        file: String,
    },
    /// Checks leafs from a single merkle tree with assets from database
    CheckTreeLeafs {
        #[arg(short, long)]
        pg_url: String,
        #[arg(short, long)]
        output: Option<String>,
        #[arg(short, long, help = "Tree pubkey")]
        tree: String,
    },
    /// Checks leafs from merkle tree from a file with assets from database
    CheckTreesLeafs {
        #[arg(short, long)]
        pg_url: String,
        #[arg(short, long)]
        output: Option<String>,
        #[arg(short, long, help = "Path to file with trees pubkeys")]
        file: String,
    },
    /// Show a tree
    ShowTree {
        #[arg(short, long, help = "Takes a single tree as a parameter to check")]
        tree: String,
    },
    /// Shows a list of trees
    ShowTrees {
        #[arg(short, long, help = "Path to file with trees pubkeys")]
        file: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // RUST_LOG=info,sqlx=warn,tree_status=debug
    env::set_var(
        env_logger::DEFAULT_FILTER_ENV,
        env::var_os(env_logger::DEFAULT_FILTER_ENV).unwrap_or_else(|| "info,sqlx=warn".into()),
    );
    env_logger::init();

    let args = Args::parse();

    let concurrency = NonZeroUsize::new(args.concurrency)
        .ok_or_else(|| anyhow::anyhow!("invalid concurrency: {}", args.concurrency))?;

    // Set up RPC interface
    let pubkeys_str = match &args.action {
        Action::CheckTree { tree, .. }
        | Action::CheckTreeLeafs { tree, .. }
        | Action::ShowTree { tree } => {
            let tree = tree.to_string();
            stream::once(async move { Ok(tree) }).boxed()
        }
        Action::CheckTrees { file, .. }
        | Action::CheckTreesLeafs { file, .. }
        | Action::ShowTrees { file } => read_lines(file).await?.boxed(),
    };

    let mut pubkeys = pubkeys_str.map(|maybe_pubkey_str| {
        maybe_pubkey_str.map_err(Into::into).and_then(|pubkey_str| {
            pubkey_str
                .parse::<Pubkey>()
                .with_context(|| format!("failed to parse pubkey: {}", &pubkey_str))
        })
    });

    match &args.action {
        Action::CheckTree { .. } | Action::CheckTrees { .. } => {
            let client = RpcClient::new(args.rpc.clone());
            let conn = args.get_pg_conn().await?;
            while let Some(maybe_pubkey) = pubkeys.next().await {
                let pubkey = maybe_pubkey?;
                info!("checking tree {pubkey}, hex: {}", hex::encode(pubkey));
                if let Err(error) = check_tree(pubkey, &client, &conn).await {
                    error!("{:?}", error);
                }
            }
        }
        Action::CheckTreeLeafs { output, .. } | Action::CheckTreesLeafs { output, .. } => {
            let conn = args.get_pg_conn().await?;
            let mut output: Option<Pin<Box<dyn AsyncWrite>>> = if let Some(output) = output {
                Some(if output == "-" {
                    Box::pin(stdout())
                } else {
                    Box::pin(
                        OpenOptions::new()
                            .write(true)
                            .create(true)
                            .truncate(true)
                            .open(output)
                            .await?,
                    )
                })
            } else {
                None
            };
            while let Some(maybe_pubkey) = pubkeys.next().await {
                let pubkey = maybe_pubkey?;
                info!("checking tree leafs {pubkey}, hex: {}", hex::encode(pubkey));
                if let Err(error) = check_tree_leafs(
                    pubkey,
                    &args.rpc,
                    concurrency,
                    args.max_retries,
                    &conn,
                    output.as_mut(),
                )
                .await
                {
                    error!("{:?}", error);
                }
            }
            if let Some(mut output) = output {
                output.flush().await?;
            }
        }
        Action::ShowTree { .. } | Action::ShowTrees { .. } => {
            while let Some(maybe_pubkey) = pubkeys.next().await {
                let pubkey = maybe_pubkey?;
                info!("showing tree {pubkey}, hex: {}", hex::encode(pubkey));
                if let Err(error) =
                    read_tree(pubkey, &args.rpc, concurrency, args.max_retries).await
                {
                    error!("{:?}", error);
                }
            }
        }
    }

    Ok(())
}

async fn check_tree(
    pubkey: Pubkey,
    client: &RpcClient,
    conn: &DatabaseConnection,
) -> anyhow::Result<()> {
    let seq = get_tree_latest_seq(pubkey, client)
        .await
        .with_context(|| format!("[{pubkey}] tree is missing from chain or error occured"))?
        .try_into()
        .unwrap();

    let indexed_seq = get_tree_max_seq(pubkey, conn)
        .await
        .with_context(|| format!("[{pubkey:?}] coundn't query tree from index"))?
        .ok_or_else(|| anyhow::anyhow!("[{pubkey}] tree missing from index"))?;

    // Check tip
    match indexed_seq.max_seq.cmp(&seq) {
        cmp::Ordering::Less => {
            error!(
                "[{pubkey}] tree not fully indexed: {} < {seq}",
                indexed_seq.max_seq
            );
        }
        cmp::Ordering::Equal => {}
        cmp::Ordering::Greater => {
            error!("[{pubkey}] indexer error: {} > {seq}", indexed_seq.max_seq);
        }
    }

    // Check completeness
    if indexed_seq.max_seq != indexed_seq.cnt_seq {
        error!(
            "[{pubkey}] tree has gaps {} != {}",
            indexed_seq.max_seq, indexed_seq.cnt_seq
        );
    }

    if indexed_seq.max_seq == seq && indexed_seq.max_seq == indexed_seq.cnt_seq {
        info!("[{:?}] indexing is complete, seq={:?}", pubkey, seq)
    } else {
        error!("[{pubkey}] indexing is failed, seq={seq} max_seq={indexed_seq:?}");
        match get_missing_seq(pubkey, seq, conn).await {
            Ok(seqs) => error!("[{pubkey}] missing seq: {seqs:?}"),
            Err(error) => error!("[{pubkey}] failed to query missing seq: {error:?}"),
        }
    }

    Ok(())
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
    let header = ConcurrentMerkleTreeHeader::try_from_slice(header_bytes)?;

    // let auth = Pubkey::find_program_address(&[address.as_ref()], &mpl_bubblegum::id()).0;

    let merkle_tree_size = merkle_tree_get_size(&header)?;
    let (tree_bytes, _canopy_bytes) = rest.split_at_mut(merkle_tree_size);

    let seq_bytes = tree_bytes[0..8].try_into().context("Error parsing bytes")?;
    Ok(u64::from_le_bytes(seq_bytes))
}

async fn get_tree_max_seq(
    tree: Pubkey,
    conn: &DatabaseConnection,
) -> Result<Option<MaxSeqItem>, DbErr> {
    let query = cl_items::Entity::find()
        .select_only()
        .filter(cl_items::Column::Tree.eq(tree.as_ref()))
        .column_as(Expr::col(cl_items::Column::Seq).max(), "max_seq")
        .column_as(Expr::cust("count(distinct seq)"), "cnt_seq")
        .build(DbBackend::Postgres);

    MaxSeqItem::find_by_statement(query).one(conn).await
}

async fn get_missing_seq(
    tree: Pubkey,
    max_seq: i64,
    conn: &DatabaseConnection,
) -> Result<Vec<MissingSeq>, DbErr> {
    let query = Statement::from_string(
        DbBackend::Postgres,
        format!(
            "
SELECT
    s.seq AS missing_seq
FROM
    generate_series(1::bigint, {}::bigint) s(seq)
WHERE
    NOT EXISTS (
        SELECT 1 FROM cl_items WHERE seq = s.seq AND tree='\\x{}'
    )",
            max_seq,
            hex::encode(tree.as_ref())
        ),
    );

    Ok(conn
        .query_all(query)
        .await?
        .iter()
        .map(|q| MissingSeq::from_query_result(q, "").unwrap())
        .collect())
}

async fn check_tree_leafs(
    pubkey: Pubkey,
    client_url: &str,
    concurrency: NonZeroUsize,
    max_retries: u8,
    conn: &DatabaseConnection,
    mut output: Option<&mut Pin<Box<dyn AsyncWrite>>>,
) -> anyhow::Result<()> {
    let (fetch_fut, mut leafs_rx) = read_tree_start(pubkey, client_url, concurrency, max_retries);
    try_join(fetch_fut, async move {
        // collect max seq per leaf index from transactions
        let mut leafs = HashMap::new();
        while let Some((_id, signature, vec)) = leafs_rx.recv().await {
            for (seq, maybe_leaf) in vec.unwrap_or_default() {
                if let Some(LeafNode {
                    index: leaf_idx,
                    leaf: _leaf,
                }) = maybe_leaf
                {
                    let entry = leafs.entry(leaf_idx).or_insert((signature, seq));
                    if entry.1 < seq {
                        *entry = (signature, seq);
                    }
                }
            }
        }

        // fetch from database in chunks
        let query = Statement::from_sql_and_values(
            DbBackend::Postgres,
            "
SELECT
    cl_items.leaf_idx, MAX(asset.seq) AS seq
FROM
    asset
INNER JOIN
    cl_items ON
        cl_items.tree = asset.tree_id AND
        cl_items.seq = asset.seq
WHERE
    asset.tree_id = $1 AND
    cl_items.leaf_idx IS NOT NULL
GROUP BY
    cl_items.leaf_idx
",
            [Value::Bytes(Some(Box::new(pubkey.as_ref().to_vec())))],
        );

        debug!("send query to database...");
        let leafs_db = conn.query_all(query).await?;

        for leaf_db in leafs_db.iter() {
            let leaf_db = AssetMaxSeq::from_query_result(leaf_db, "").unwrap();
            match leafs.remove(&leaf_db.leaf_idx) {
                Some((signature, seq)) => {
                    if leaf_db.seq != seq as i64 {
                        error!(
                            "leaf index {}: invalid seq {} vs {} (db vs blockchain, tx={:?})",
                            leaf_db.leaf_idx, leaf_db.seq, seq, signature
                        );
                    }
                }
                None => {
                    error!("leaf index {}: not found in blockchain", leaf_db.leaf_idx);
                }
            }
        }
        for (leaf_idx, (signature, seq)) in leafs.into_iter() {
            error!("leaf index {leaf_idx}: not found in db, seq {seq} tx={signature:?}");
            if let Some(output) = output.as_mut() {
                let _ = output.write(format!("{signature}\n").as_bytes()).await?;
            }
        }

        Ok(())
    })
    .await
    .map(|_| ())
}

// Fetches all the transactions referencing a specific trees
async fn read_tree(
    pubkey: Pubkey,
    client_url: &str,
    concurrency: NonZeroUsize,
    max_retries: u8,
) -> anyhow::Result<()> {
    fn print_seqs(id: usize, sig: Signature, seqs: Option<Vec<(u64, MaybeLeafNode)>>) {
        for (seq, leaf_idx) in seqs.unwrap_or_default() {
            let leaf_idx = leaf_idx.map(|v| v.index.to_string()).unwrap_or_default();
            info!("{seq} {leaf_idx} {sig} {id}");
        }
    }

    let (fetch_fut, mut print_rx) = read_tree_start(pubkey, client_url, concurrency, max_retries);
    try_join(fetch_fut, async move {
        let mut next_id = 0;
        let mut map = HashMap::new();

        while let Some((id, sig, seqs)) = print_rx.recv().await {
            map.insert(id, (sig, seqs));

            if let Some((sig, seqs)) = map.remove(&next_id) {
                print_seqs(next_id, sig, seqs);
                next_id += 1;
            }
        }

        let mut vec = map.into_iter().collect::<Vec<_>>();
        vec.sort_by_key(|(id, _)| *id);
        for (id, (sig, seqs)) in vec.into_iter() {
            print_seqs(id, sig, seqs);
        }

        Ok(())
    })
    .await
    .map(|_| ())
}

#[allow(clippy::type_complexity)]
fn read_tree_start(
    pubkey: Pubkey,
    client_url: &str,
    concurrency: NonZeroUsize,
    max_retries: u8,
) -> (
    BoxFuture<'static, anyhow::Result<()>>,
    mpsc::UnboundedReceiver<(usize, Signature, Option<Vec<(u64, MaybeLeafNode)>>)>,
) {
    let sig_id = Arc::new(AtomicUsize::new(0));
    let rx_sig = Arc::new(Mutex::new(find_signatures(
        pubkey,
        RpcClient::new(client_url.to_owned()),
        2_000,
    )));

    let (tx, rx) = mpsc::unbounded_channel();
    let tx = Arc::new(tx);

    let fetch_futs = (0..concurrency.get())
        .map(|_| {
            let sig_id = Arc::clone(&sig_id);
            let rx_sig = Arc::clone(&rx_sig);
            let client = RpcClient::new(client_url.to_owned());
            let tx = Arc::clone(&tx);
            async move {
                loop {
                    let mut lock = rx_sig.lock().await;
                    let maybe_msg = lock.recv().await;
                    let id = sig_id.fetch_add(1, Ordering::SeqCst);
                    if id > 0 && id % 10 == 0 {
                        debug!("received {} transactions", id);
                    }
                    drop(lock);
                    match maybe_msg {
                        Some(maybe_sig) => {
                            let signature = maybe_sig?;
                            let mut map = process_tx(signature, &client, max_retries).await?;
                            let _ = tx.send((id, signature, map.remove(&pubkey)));
                        }
                        None => return Ok::<(), anyhow::Error>(()),
                    }
                }
            }
        })
        .collect::<Vec<_>>();
    drop(tx);

    (try_join_all(fetch_futs).map_ok(|_| ()).boxed(), rx)
}

// Process and individual transaction, fetching it and reading out the sequence numbers
async fn process_tx(
    signature: Signature,
    client: &RpcClient,
    max_retries: u8,
) -> anyhow::Result<HashMap<Pubkey, Vec<(u64, MaybeLeafNode)>>> {
    const CONFIG: RpcTransactionConfig = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Base64),
        commitment: Some(CommitmentConfig {
            commitment: CommitmentLevel::Finalized,
        }),
        max_supported_transaction_version: Some(0),
    };

    let tx: EncodedConfirmedTransactionWithStatusMeta = rpc_send_with_retries(
        client,
        RpcRequest::GetTransaction,
        serde_json::json!([signature.to_string(), CONFIG]),
        max_retries,
        signature,
    )
    .await?;
    parse_tx_sequence(tx).map_err(Into::into)
}

// Parse the trasnaction data
fn parse_tx_sequence(
    tx: EncodedConfirmedTransactionWithStatusMeta,
) -> Result<HashMap<Pubkey, Vec<(u64, MaybeLeafNode)>>, ParseError> {
    let mut seq_updates = HashMap::<Pubkey, Vec<(u64, MaybeLeafNode)>>::new();

    // Get `UiTransaction` out of `EncodedTransactionWithStatusMeta`.
    let meta: UiTransactionStatusMeta = tx.transaction.meta.ok_or(ParseError::TransactionMeta)?;

    // See https://github.com/ngundotra/spl-ac-seq-parse/blob/main/src/main.rs
    if let OptionSerializer::Some(inner_instructions_vec) = meta.inner_instructions.as_ref() {
        let transaction: VersionedTransaction = tx
            .transaction
            .transaction
            .decode()
            .ok_or(ParseError::Transaction)?;

        // Add the account lookup stuff
        let mut account_keys = transaction.message.static_account_keys().to_vec();
        if let OptionSerializer::Some(loaded_addresses) = meta.loaded_addresses {
            for pubkey in loaded_addresses.writable.iter() {
                account_keys.push(Pubkey::from_str(pubkey)?);
            }
            for pubkey in loaded_addresses.readonly.iter() {
                account_keys.push(Pubkey::from_str(pubkey)?);
            }
        }

        for inner_ixs in inner_instructions_vec.iter() {
            for inner_ix in inner_ixs.instructions.iter() {
                if let solana_transaction_status::UiInstruction::Compiled(instr) = inner_ix {
                    if let Some(program) = account_keys.get(instr.program_id_index as usize) {
                        if *program == spl_noop::id() {
                            let data = bs58::decode(&instr.data)
                                .into_vec()
                                .map_err(ParseError::Instruction)?;

                            if let Ok(AccountCompressionEvent::ChangeLog(cl_data)) =
                                AccountCompressionEvent::try_from_slice(&data)
                            {
                                let ChangeLogEvent::V1(cl_data) = cl_data;
                                let leaf = cl_data.path.get(0).map(|node| LeafNode {
                                    leaf: node.node.to_vec(),
                                    index: node_idx_to_leaf_idx(
                                        node.index as i64,
                                        cl_data.path.len() as u32 - 1,
                                    ),
                                });
                                seq_updates
                                    .entry(cl_data.id)
                                    .or_default()
                                    .push((cl_data.seq, leaf));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(seq_updates)
}

fn node_idx_to_leaf_idx(index: i64, tree_height: u32) -> i64 {
    index - 2i64.pow(tree_height)
}
