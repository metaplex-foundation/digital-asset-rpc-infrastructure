use {
    anyhow::Context,
    borsh::BorshDeserialize,
    clap::{arg, Parser, Subcommand},
    digital_asset_types::dao::cl_items,
    sea_orm::{
        sea_query::Expr, ColumnTrait, DatabaseConnection, DbBackend, DbErr, EntityTrait,
        FromQueryResult, QueryFilter, QuerySelect, QueryTrait, SqlxPostgresConnector,
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
        ConnectOptions, PgPool,
    },
};

#[derive(Debug, FromQueryResult, Clone)]
struct MaxSeqItem {
    max_seq: i64,
    cnt_seq: i64,
}

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, default_value_t = { "https://index.rpcpool.com/a4d23a00546272efeba9843a4ae4".to_owned() })]
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
                            eprintln!("[{:?}] indexing is failed, seq={:?} max_seq={:?}", pubkey, seq, indexed_seq)
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

    // get signatures
    /*let sigs = client
                .get_signatures_for_address_with_config(
                    &address,
                    GetConfirmedSignaturesForAddress2Config {
                        before: None,
                        until: None,
                        commitment: Some(CommitmentConfig::confirmed()),
                        ..GetConfirmedSignaturesForAddress2Config::default()
                    },
                )
                .await
                .map_err(|e| e.to_string())?;

    if sigs.is_empty() {
        return Ok(0);
    }

    let mut first_sig = None;
    for sig in sigs.iter() {
        if sig.confirmation_status.is_none() || sig.err.is_some() {
            continue;
        }
        // Break on the first non err and confirmed transaction
        first_sig = Some(sig);
        break;
    };

    if let Some(first_sig) = first_sig {
        let sig_str = Signature::from_str(&first_sig.signature).unwrap();
        println!("first sig: {}", sig_str);

        let tx = client
            .get_transaction_with_config(
                &sig_str,
                solana_client::rpc_config::RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::Base64),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: Some(0),
                },
            )
            .await
            .unwrap();

        // Being a bit lazy and encoding this flatbuffers. Almost certainly we don't have to
        let fbb = flatbuffers::FlatBufferBuilder::new();
        let fbb = seralize_encoded_transaction_with_status(fbb, tx);

        match fbb {
            Ok(fb_tx) => {
                let bytes = fb_tx.finished_data();

                println!("tx: {:?}", bytes);
            },
            Err(e) => {
                println!("err: {:?}", e);
            }
        }
    }*/
    /*let tx = Signature::from_str(sig).unwrap();
            .get_transaction_with_config(
                &sig,
                solana_client::rpc_config::RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::Base64),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: Some(0),
                },
            )
            .await
            .unwrap();
    */
}

/*
pub async fn handle_transaction<'a>(
    &self,
    tx: &'a TransactionInfo<'a>,
) -> Result<(), IngesterError> {
    info!("Handling Transaction: {:?}", tx.signature());
    let instructions = self.break_transaction(&tx);
    let accounts = tx.account_keys().unwrap_or_default();
    let slot = tx.slot();
    let mut keys: Vec<FBPubkey> = Vec::with_capacity(accounts.len());
    for k in accounts.into_iter() {
        keys.push(*k);
    }
    let mut not_impl = 0;
    let ixlen = instructions.len();
    debug!("Instructions: {}", ixlen);
    let contains = instructions
        .iter()
        .filter(|(ib, _inner)| ib.0 .0.as_ref() == mpl_bubblegum::id().as_ref());
    debug!("Instructions bgum: {}", contains.count());
    for (outer_ix, inner_ix) in instructions {
        let (program, instruction) = outer_ix;
        let ix_accounts = instruction.accounts().unwrap().iter().collect::<Vec<_>>();
        let ix_account_len = ix_accounts.len();
        let max = ix_accounts.iter().max().copied().unwrap_or(0) as usize;
        if keys.len() < max {
            return Err(IngesterError::DeserializationError(
                "Missing Accounts in Serialized Ixn/Txn".to_string(),
            ));
        }
        let ix_accounts =
            ix_accounts
                .iter()
                .fold(Vec::with_capacity(ix_account_len), |mut acc, a| {
                    if let Some(key) = keys.get(*a as usize) {
                        acc.push(*key);
                    }
                    acc
                });
        let ix = InstructionBundle {
            txn_id: "",
            program,
            instruction: Some(instruction),
            inner_ix,
            keys: ix_accounts.as_slice(),
            slot,
        };

        if let Some(program) = self.match_program(&ix.program) {
            debug!("Found a ix for program: {:?}", program.key());
            let result = program.handle_instruction(&ix)?;
            let concrete = result.result_type();
            match concrete {
                ProgramParseResult::Bubblegum(parsing_result) => {
                    handle_bubblegum_instruction(
                        parsing_result,
                        &ix,
                        &self.storage,
                        &self.task_sender,
                    )
                    .await?;
                }
                _ => {
                    not_impl += 1;
                }
            };
        }
    }

    if not_impl == ixlen {
        debug!("Not imple");
        return Err(IngesterError::NotImplemented);
    }
    Ok(())
}
*/
