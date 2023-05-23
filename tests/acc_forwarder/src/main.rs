use {
    anyhow::Context,
    clap::Parser,
    figment::{map, value::Value},
    futures::{future::try_join_all, stream::StreamExt},
    log::{info, warn},
    mpl_token_metadata::{pda::find_metadata_account, state::Metadata},
    plerkle_messenger::{MessengerConfig, ACCOUNT_STREAM},
    plerkle_serialization::{
        serializer::serialize_account, solana_geyser_plugin_interface_shims::ReplicaAccountInfoV2,
    },
    solana_account_decoder::{UiAccount, UiAccountEncoding},
    solana_client::{
        nonblocking::rpc_client::RpcClient,
        rpc_config::{RpcAccountInfoConfig, RpcTransactionConfig},
        rpc_request::RpcRequest,
        rpc_response::{Response as RpcResponse, RpcTokenAccountBalance},
    },
    solana_sdk::{
        account::Account,
        borsh::try_from_slice_unchecked,
        commitment_config::{CommitmentConfig, CommitmentLevel},
        pubkey::Pubkey,
        signature::Signature,
    },
    solana_transaction_status::{
        EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiInstruction, UiMessage,
        UiParsedInstruction, UiTransactionEncoding,
    },
    std::{collections::HashSet, env, str::FromStr, sync::Arc},
    tokio::sync::Mutex,
    txn_forwarder::{find_signatures, read_lines, rpc_send_with_retries},
};

#[derive(Parser)]
#[command(next_line_help = true)]
struct Args {
    #[arg(long)]
    redis_url: String,
    #[arg(long)]
    rpc_url: String,
    #[command(subcommand)]
    action: Action,
}

#[derive(clap::Subcommand, Clone)]
enum Action {
    Single {
        #[arg(long)]
        account: String,
    },
    Scenario {
        #[arg(long)]
        scenario_file: String,
    },
    Mint {
        // puts in mint, token, and metadata account
        #[arg(long)]
        mint: String,
    },
    Collection {
        #[arg(long)]
        collection: String,
        #[arg(long, default_value_t = 25)]
        concurrency: usize,
    },
}

#[derive(Debug)]
struct CollectionTransactionInfo {
    pub program_id: String,
    pub accounts: Vec<String>,
    pub data: String,
}

impl CollectionTransactionInfo {
    fn is_valid(&self) -> bool {
        self.program_id == mpl_token_metadata::ID.to_string()
            && (self.data == "S" || self.data == "K")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env::set_var(
        env_logger::DEFAULT_FILTER_ENV,
        env::var_os(env_logger::DEFAULT_FILTER_ENV).unwrap_or_else(|| "info".into()),
    );
    env_logger::init();

    let args = Args::parse();
    let config_wrapper = Value::from(map! {
        "redis_connection_str" => args.redis_url,
        "pipeline_size_bytes" => 1u128.to_string(),
    });
    let config = config_wrapper.into_dict().unwrap();
    let messenger_config = MessengerConfig {
        messenger_type: plerkle_messenger::MessengerType::Redis,
        connection_config: config,
    };
    let mut messenger = plerkle_messenger::select_messenger(messenger_config)
        .await
        .unwrap();
    messenger.add_stream(ACCOUNT_STREAM).await.unwrap();
    messenger
        .set_buffer_size(ACCOUNT_STREAM, 10000000000000000)
        .await;
    let messenger = Arc::new(Mutex::new(messenger));

    let client = RpcClient::new(args.rpc_url.clone());

    match args.action {
        Action::Single { account } => {
            let pubkey = Pubkey::from_str(&account)
                .with_context(|| format!("failed to parse account {account}"))?;
            fetch_and_send_account(pubkey, &client, &messenger).await?;
        }
        Action::Scenario { scenario_file } => {
            let mut accounts = read_lines(&scenario_file).await?;
            while let Some(maybe_account) = accounts.next().await {
                let pubkey = maybe_account?.parse()?;
                fetch_and_send_account(pubkey, &client, &messenger).await?;
            }
        }
        Action::Mint { mint } => {
            let mint =
                Pubkey::from_str(&mint).with_context(|| format!("failed to parse mint {mint}"))?;
            let metadata_account = find_metadata_account(&mint).0;
            let token_account = get_token_largest_account(&client, mint).await;

            match token_account {
                Ok(token_account) => {
                    for pubkey in &[mint, metadata_account, token_account] {
                        fetch_and_send_account(*pubkey, &client, &messenger).await?;
                    }
                }
                Err(e) => warn!("Failed to find mint account: {:?}", e),
            }
        }
        Action::Collection {
            collection,
            concurrency,
        } => {
            let metadata_accounts = Arc::new(Mutex::new(HashSet::new()));

            let collection = Pubkey::from_str(&collection)
                .with_context(|| format!("failed to parse collection {collection}"))?;
            let stream = Arc::new(Mutex::new(find_signatures(collection, client, 2_000)));

            try_join_all((0..concurrency).map(|_| {
                let metadata_accounts = Arc::clone(&metadata_accounts);
                let stream = Arc::clone(&stream);
                let client = RpcClient::new(args.rpc_url.clone());
                let messenger = Arc::clone(&messenger);
                async move {
                    loop {
                        let mut locked = stream.lock().await;
                        let maybe_signature = locked.recv().await;
                        drop(locked);

                        let mut txinfo = match maybe_signature {
                            Some(signature) => {
                                match collection_get_tx_info(&client, signature?).await? {
                                    Some(txinfo) => txinfo,
                                    None => continue,
                                }
                            }
                            None => return Ok::<(), anyhow::Error>(()),
                        };

                        let account = txinfo.accounts.remove(0);
                        let account = Pubkey::from_str(&account)
                            .with_context(|| format!("failed to parse account {account}"))?;

                        let mut locked = metadata_accounts.lock().await;
                        let inserted = locked.insert(account);
                        drop(locked);

                        if inserted {
                            match fetch_metadata_and_send_accounts(account, &client, &messenger)
                                .await
                            {
                                Ok(_) => info!("Uploaded {:?}", account),
                                Err(e) => warn!("Could not insert {:?}: {:?}", account, e),
                            }
                        }
                    }
                }
            }))
            .await?;
        }
    }

    Ok(())
}

// https://github.com/metaplex-foundation/get-collection/blob/main/get-collection-rs/src/crawl.rs
// fetch tx and filter
async fn collection_get_tx_info(
    client: &RpcClient,
    signature: Signature,
) -> anyhow::Result<Option<CollectionTransactionInfo>> {
    const CONFIG: RpcTransactionConfig = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::JsonParsed),
        commitment: Some(CommitmentConfig {
            commitment: CommitmentLevel::Confirmed,
        }),
        max_supported_transaction_version: Some(u8::MAX),
    };

    let tx: EncodedConfirmedTransactionWithStatusMeta = rpc_send_with_retries(
        client,
        RpcRequest::GetTransaction,
        serde_json::json!([signature.to_string(), CONFIG]),
        3,
        signature,
    )
    .await?;
    info!("fetch transaction {signature:?}");

    let tx = match tx.transaction.transaction {
        EncodedTransaction::Json(tx) => tx,
        _ => anyhow::bail!("invalid encoded tx: {signature}"),
    };

    let mut txinfo = None;
    match tx.message {
        UiMessage::Parsed(value) => {
            for ix in value.instructions {
                match ix {
                    UiInstruction::Parsed(ix) => match ix {
                        UiParsedInstruction::PartiallyDecoded(ix) => {
                            txinfo.replace(CollectionTransactionInfo {
                                program_id: ix.program_id,
                                accounts: ix.accounts,
                                data: ix.data,
                            });
                        }
                        // skip system instructions
                        UiParsedInstruction::Parsed(_ix) => {}
                    },
                    UiInstruction::Compiled(ix) => {
                        let accounts: Vec<String> = ix
                            .accounts
                            .chunks(32)
                            .map(|x| bs58::encode(x).into_string())
                            .collect();

                        txinfo.replace(CollectionTransactionInfo {
                            program_id: accounts[ix.program_id_index as usize].clone(),
                            accounts,
                            data: ix.data,
                        });
                    }
                }
            }
        }
        UiMessage::Raw(value) => {
            for ix in value.instructions {
                let accounts: Vec<String> = ix
                    .accounts
                    .chunks(32)
                    .map(|x| bs58::encode(x).into_string())
                    .collect();

                txinfo.replace(CollectionTransactionInfo {
                    program_id: accounts[ix.program_id_index as usize].clone(),
                    accounts,
                    data: ix.data,
                });
            }
        }
    };
    Ok(match txinfo {
        Some(txinfo) if txinfo.is_valid() => Some(txinfo),
        _ => None,
    })
}

// fetch metadata account and send mint account to redis
async fn fetch_metadata_and_send_accounts(
    pubkey: Pubkey,
    client: &RpcClient,
    messenger: &Arc<Mutex<Box<dyn plerkle_messenger::Messenger>>>,
) -> anyhow::Result<()> {
    let (account, _slot) = fetch_account(pubkey, client).await?;
    let metadata: Metadata = try_from_slice_unchecked(&account.data)
        .with_context(|| anyhow::anyhow!("failed to parse data for metadata account {pubkey}"))?;

    info!("Fetching token largest accounts: {:?}", metadata.mint);
    let token_account = get_token_largest_account(client, metadata.mint).await?;

    for pubkey in &[metadata.mint, pubkey, token_account] {
        fetch_and_send_account(*pubkey, client, messenger).await?;
    }
    Ok(())
}

// returns largest (NFT related) token account belonging to mint
async fn get_token_largest_account(client: &RpcClient, mint: Pubkey) -> anyhow::Result<Pubkey> {
    let response: RpcResponse<Vec<RpcTokenAccountBalance>> = rpc_send_with_retries(
        client,
        RpcRequest::Custom {
            method: "getTokenLargestAccounts",
        },
        serde_json::json!([mint.to_string(),]),
        3,
        mint,
    )
    .await?;

    match response.value.first() {
        Some(account) => Pubkey::from_str(&account.address)
            .with_context(|| format!("failed to parse account for mint {mint}")),
        None => anyhow::bail!("no accounts for mint {mint}: burned nft?"),
    }
}

// fetch account and slot with retries
async fn fetch_account(pubkey: Pubkey, client: &RpcClient) -> anyhow::Result<(Account, u64)> {
    const CONFIG: RpcAccountInfoConfig = RpcAccountInfoConfig {
        encoding: Some(UiAccountEncoding::Base64Zstd),
        commitment: Some(CommitmentConfig {
            commitment: CommitmentLevel::Confirmed,
        }),
        data_slice: None,
        min_context_slot: None,
    };

    let response: RpcResponse<Option<UiAccount>> = rpc_send_with_retries(
        client,
        RpcRequest::GetAccountInfo,
        serde_json::json!([pubkey.to_string(), CONFIG]),
        3,
        pubkey,
    )
    .await
    .with_context(|| format!("failed to get account {pubkey}"))?;

    let account: Account = response
        .value
        .ok_or_else(|| anyhow::anyhow!("failed to get account {pubkey}"))?
        .decode()
        .ok_or_else(|| anyhow::anyhow!("failed to parse account {pubkey}"))?;

    Ok((account, response.context.slot))
}

// fetch account from node and send it to redis
async fn fetch_and_send_account(
    pubkey: Pubkey,
    client: &RpcClient,
    messenger: &Arc<Mutex<Box<dyn plerkle_messenger::Messenger>>>,
) -> anyhow::Result<()> {
    let (account, slot) = fetch_account(pubkey, client).await?;
    send_account(pubkey, account, slot, messenger).await
}

// send account data to redis
async fn send_account(
    pubkey: Pubkey,
    account: Account,
    slot: u64,
    messenger: &Arc<Mutex<Box<dyn plerkle_messenger::Messenger>>>,
) -> anyhow::Result<()> {
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

    let fbb = serialize_account(fbb, &account_info, slot, is_startup);
    let bytes = fbb.finished_data();

    messenger.lock().await.send(ACCOUNT_STREAM, bytes).await?;
    info!("sent account {} to stream", pubkey);

    Ok(())
}
