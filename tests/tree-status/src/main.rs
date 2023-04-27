use {
    anyhow::Context,
    borsh::BorshDeserialize,
    clap::{arg, Parser, Subcommand},
    digital_asset_types::dao::cl_items,
    sea_orm::{
        sea_query::Expr, ColumnTrait, DatabaseConnection, DbBackend, DbErr, EntityTrait,
        FromQueryResult, QueryFilter, QuerySelect, QueryTrait, SqlxPostgresConnector,
        ConnectionTrait, Statement
    },
    // plerkle_serialization::serializer::seralize_encoded_transaction_with_status,
    // solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config,
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey},
    // solana_sdk::signature::Signature,
    // solana_transaction_status::UiTransactionEncoding,
    spl_account_compression::state::{
        merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
    },
    sqlx::{
        postgres::{PgConnectOptions, PgPoolOptions},
        ConnectOptions, PgPool
    },
};

#[derive(Debug, FromQueryResult, Clone)]
struct MaxSeqItem {
    max_seq: i64,
    cnt_seq: i64,
}

#[derive(Debug, FromQueryResult, Clone)]
struct MissingSeq {
    missing_seq: i64,
}

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long)]
    rpc_url: String,

    #[arg(short, long)]
    pg_url: String,

    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand, Clone)]
enum Action {
    /// Checks a single merkle tree to check if it;s fully indexed
    CheckTree {
        #[arg(short, long, help = "Takes a single pubkey as a parameter to check")]
        key: String,
    },
    /// Checks a list of merkle trees to check if they're fully indexed
    CheckTrees {
        #[arg(
            short,
            long,
            help = "Takes a path to a file with pubkeys as a parameter to check"
        )]
        file: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Set up db connection
    let url = args.pg_url;
    let mut options: PgConnectOptions = url.parse().unwrap();

    // Create postgres pool
    let pool = PgPoolOptions::new()
        .min_connections(2)
        .max_connections(10)
        .connect_with(options)
        .await
        .unwrap();

    // Create new postgres connection
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());

    // Set up RPC interface
    let client = RpcClient::new(args.rpc_url);

    let pubkeys = match args.action {
        Action::CheckTree { key } => vec![key],
        Action::CheckTrees { file } => tokio::fs::read_to_string(&file)
            .await
            .with_context(|| format!("failed to read file with keys: {:?}", file))?
            .split('\n')
            .filter_map(|x| {
                let x = x.trim();
                (!x.is_empty()).then(|| x.to_string())
            })
            .collect(),
    };

    for pubkey in pubkeys {
        match pubkey.parse() {
            Ok(pubkey) => {
                let seq = get_tree_latest_seq(pubkey, &client).await;
                //println!("seq for pubkey {:?}: {:?}", pubkey, seq);
                if seq.is_err() {
                   eprintln!("[{:?}] tree is missing from chain or error occurred: {:?}", pubkey, seq);
                    continue;
                }

                let seq = seq.unwrap();

                let fetch_seq = get_tree_max_seq(&pubkey.to_bytes(), &conn).await;
                if fetch_seq.is_err() {
                    eprintln!("[{:?}] couldn't query tree from index: {:?}", pubkey, fetch_seq);
                    continue;
                }
                match fetch_seq.unwrap() {
                    Some(indexed_seq) => {
                        let mut indexing_successful = false;
                        // Check tip 
                        if indexed_seq.max_seq > seq.try_into().unwrap() {
                            eprintln!("[{:?}] indexer error: {:?} > {:?}", pubkey, indexed_seq.max_seq, seq);
                        } else if indexed_seq.max_seq < seq.try_into().unwrap() {
                            eprintln!(
                                "[{:?}] tree not fully indexed: {:?} < {:?}",
                                pubkey, indexed_seq.max_seq, seq
                            );
                        } else {
                            indexing_successful = true;
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
                            eprintln!("[{:?}] indexing is failed, seq={:?} max_seq={:?}", pubkey, seq, indexed_seq);
                            let ret = get_missing_seq(&pubkey.to_bytes(), seq.try_into().unwrap(), &conn).await;
                            if ret.is_err() {
                                eprintln!("[{:?}] failed to query missing seq: {:?}", pubkey, ret);
                            } else {
                                let ret = ret.unwrap();
                                eprintln!("[{:?}] missing seq: {:?}", pubkey, ret);
                            }
                        }
                    },
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
) ->  Result<Vec<MissingSeq>, DbErr> {
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
