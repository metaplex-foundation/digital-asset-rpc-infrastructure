mod utils;

use std::{str::FromStr, sync::Arc};

use clap::Parser;
use figment::{util::map, value::Value};

use async_recursion::async_recursion;

use plerkle_messenger::MessengerConfig;
use plerkle_serialization::serializer::seralize_encoded_transaction_with_status;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature,
};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta,
    UiTransactionEncoding,
};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use utils::Siggrabbenheimer;

#[derive(Parser)]
#[command(next_line_help = true)]
struct Cli {
    #[arg(long)]
    redis_url: String,
    #[arg(long)]
    rpc_url: String,
    #[arg(long, short, default_value_t = 3)]
    max_retries: u8,
    #[command(subcommand)]
    action: Action,
}
#[derive(clap::Subcommand, Clone)]
enum Action {
    Single {
        #[arg(long)]
        txn: String,
    },
    Address {
        #[arg(long)]
        address: String,
        #[arg(long)]
        include_failed: Option<bool>,
    },
    Scenario {
        #[arg(long)]
        scenario_file: String,
    },
}
const STREAM: &str = "TXN";

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let config_wrapper = Value::from(map! {
    "redis_connection_str" => cli.redis_url,
    "pipeline_size_bytes" => 1u128.to_string(),
     });
    let config = config_wrapper.into_dict().unwrap();
    let messenenger_config = MessengerConfig {
        messenger_type: plerkle_messenger::MessengerType::Redis,
        connection_config: config,
    };
    let mut messenger = plerkle_messenger::select_messenger(messenenger_config)
        .await
        .unwrap();
    messenger.add_stream(STREAM).await.unwrap();
    messenger.add_stream("ACC").await.unwrap();
    messenger.set_buffer_size(STREAM, 10000000000000000).await;
    let messenger = Arc::new(Mutex::new(messenger));

    // TODO allow txn piping to stdin
    let client = RpcClient::new(cli.rpc_url.clone());

    let cmd = cli.action;

    match cmd {
        Action::Single { txn } => send_txn(&txn, &client, cli.max_retries, messenger).await,
        Action::Address {
            include_failed,
            address,
        } => {
            println!("Sending address");
            send_address(
                &address,
                cli.rpc_url,
                messenger,
                include_failed.unwrap_or(false),
                cli.max_retries,
            )
            .await;
        }
        Action::Scenario { scenario_file } => {
            let scenario = std::fs::read_to_string(scenario_file).unwrap();
            let scenario: Vec<String> = scenario.lines().map(|s| s.to_string()).collect();
            let mut tasks = Vec::new();
            for txn in scenario {
                let client = RpcClient::new(cli.rpc_url.clone());
                let messenger = Arc::clone(&messenger);
                tasks.push(tokio::spawn(async move {
                    send_txn(&txn, &client, cli.max_retries, messenger).await;
                }));
            }
            for task in tasks {
                task.await.unwrap();
            }
        }
    }
}

pub async fn send_address(
    address: &str,
    client_url: String,
    messenger: Arc<Mutex<Box<dyn plerkle_messenger::Messenger>>>,
    failed: bool,
    max_retries: u8,
) {
    let client1 = RpcClient::new(client_url.clone());
    let pub_addr = Pubkey::from_str(address).unwrap();
    // This takes a param failed but it excludes all failed TXs
    let mut sig = Siggrabbenheimer::new(client1, pub_addr, failed);
    let mut tasks = Vec::new();
    while let Some(s) = sig.next().await {
        let client_url = client_url.clone();
        let messenger = Arc::clone(&messenger);
        tasks.push(tokio::spawn(async move {
            let client2 = RpcClient::new(client_url.clone());
            let messenger = Arc::clone(&messenger);
            send_txn(&s, &client2, max_retries, messenger).await;
        }))
    }
    for task in tasks {
        task.await.unwrap();
    }
}

#[async_recursion]
pub async fn send_txn(
    sig_str: &str,
    client: &RpcClient,
    retries: u8,
    messenger: Arc<Mutex<Box<dyn plerkle_messenger::Messenger>>>,
) {
    let sig = Signature::from_str(sig_str).unwrap();
    let txn = client
        .get_transaction_with_config(
            &sig,
            solana_client::rpc_config::RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Base64),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            },
        )
        .await;

    match txn {
        Ok(txn) => {
            send(&sig, txn, messenger).await;
        }
        Err(e) => {
            if retries > 0 {
                println!("Retrying transaction {} retry no {}: {}", sig, retries, e);
                send_txn(sig_str, client, retries - 1, messenger).await;
            } else {
                println!("Could not load transaction {}: {}", sig, e);
                eprintln!("{}", sig);
            }
        }
    }
}

pub async fn send(
    sig: &Signature,
    txn: EncodedConfirmedTransactionWithStatusMeta,
    messenger: Arc<Mutex<Box<dyn plerkle_messenger::Messenger>>>,
) {
    let fbb = flatbuffers::FlatBufferBuilder::new();
    let fbb = seralize_encoded_transaction_with_status(fbb, txn);

    match fbb {
        Ok(fb_tx) => {
            let bytes = fb_tx.finished_data();
            messenger.lock().await.send(STREAM, bytes).await.unwrap();
            println!("Sent txn to stream {}", sig);
        }
        Err(e) => {
            println!("Failed to send txn {} to stream ({:?})", sig, e);
        }
    }
}
