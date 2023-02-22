mod utils;
use std::{collections::BTreeMap, str::FromStr, sync::Arc};

use clap::Parser;
use figment::{util::map, value::Value};

use plerkle_messenger::MessengerConfig;
use plerkle_serialization::serializer::seralize_encoded_transaction_with_status;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransactionWithStatusMeta,
    UiTransactionEncoding,
};
use tokio_stream::StreamExt;
use utils::Siggrabbenheimer;
#[derive(Parser)]
#[command(next_line_help = true)]
struct Cli {
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
const MAX_CACHE_COST: i64 = 32;
const BLOCK_CACHE_DURATION: u64 = 172800;
const BLOCK_CACHE_SIZE: usize = 300_000;

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

    // TODO allow txn piping to stdin
    let client = RpcClient::new(cli.rpc_url.clone());

    let cmd = cli.action;

    match cmd {
        Action::Single { txn } => send_txn(&txn, &client, &mut messenger).await,
        Action::Address {
            include_failed,
            address,
        } => {
            println!("Sending address");
            send_address(
                &address,
                cli.rpc_url,
                &mut messenger,
                include_failed.unwrap_or(false),
            )
            .await;
        }
        Action::Scenario { scenario_file } => {
            let scenario = std::fs::read_to_string(scenario_file).unwrap();
            let scenario: Vec<String> = scenario.lines().map(|s| s.to_string()).collect();
            for txn in scenario {
                send_txn(&txn, &client, &mut messenger).await;
            }
        }
    }
}

pub async fn send_address(
    address: &str,
    client_url: String,
    messenger: &mut Box<dyn plerkle_messenger::Messenger>,
    failed: bool,
) {
    let client1 = RpcClient::new(client_url.clone());
    let pub_addr = Pubkey::from_str(address).unwrap();
    let mut sig = Siggrabbenheimer::new(client1, pub_addr, failed);
    let client2 = RpcClient::new(client_url);
    while let Some(s) = sig.next().await {
        send_txn(&s, &client2, messenger).await;
    }
}

pub async fn send_txn(
    txn: &str,
    client: &RpcClient,
    messenger: &mut Box<dyn plerkle_messenger::Messenger>,
) {
    let sig = Signature::from_str(txn).unwrap();
    let txn = client
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

    send(txn, messenger).await
}

pub async fn send(
    txn: EncodedConfirmedTransactionWithStatusMeta,
    messenger: &mut Box<dyn plerkle_messenger::Messenger>,
) {
    let fbb = flatbuffers::FlatBufferBuilder::new();
    let fbb = seralize_encoded_transaction_with_status(fbb, txn).unwrap();
    let bytes = fbb.finished_data();

    messenger.send(STREAM, bytes).await.unwrap();
    println!("Sent txn to stream");
}
