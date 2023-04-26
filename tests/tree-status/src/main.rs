use {
    anyhow::Context,
    borsh::BorshDeserialize,
    clap::{arg, Parser, Subcommand},
    // plerkle_serialization::serializer::seralize_encoded_transaction_with_status,
    // solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config,
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey},
    // solana_sdk::signature::Signature,
    // solana_transaction_status::UiTransactionEncoding,
    spl_account_compression::state::{
        merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
    },
};

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, default_value_t = { "a4d23a00546272efeba9843a4ae4".to_owned() })]
    access_key: String,

    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand, Clone)]
enum Action {
    CheckTree {
        #[arg(short, long, default_value_t = { "8wKvdzBu2kEG5T3maJBX8m2gLs4XFavXzCKiZcGVeS8T".to_owned() })]
        key: String,
    },
    CheckTrees {
        #[arg(short, long)]
        file: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let client = RpcClient::new(format!("https://index.rpcpool.com/{}", args.access_key));
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
                println!("seq for pubkey {:?}: {:?}", pubkey, seq);
            }
            Err(error) => {
                eprintln!("failed to parse pubkey {:?}, reason: {:?}", pubkey, error);
            }
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
