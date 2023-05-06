use {
    anchor_client::anchor_lang::AnchorDeserialize,
    async_recursion::async_recursion,
    clap::Parser,
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_sdk::{
        commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature,
        transaction::VersionedTransaction,
    },
    solana_transaction_status::{
        option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
        UiTransactionEncoding, UiTransactionStatusMeta,
    },
    spl_account_compression::{AccountCompressionEvent, ChangeLogEvent},
    std::str::FromStr,
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

#[derive(Parser)]
#[command(next_line_help = true)]
struct Cli {
    #[arg(long)]
    rpc_url: String,
    #[arg(long)]
    address: String,
    #[arg(long, short, default_value_t = 3)]
    max_retries: u8,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    read_tree(cli.address, cli.rpc_url, false, cli.max_retries).await;
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
            for inner_ix in inner_ixs.instructions.iter() {
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
