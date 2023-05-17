use {
    clap::Parser,
    figment::{map, value::Value},
    mpl_token_metadata::pda::find_metadata_account,
    plerkle_messenger::MessengerConfig,
    plerkle_serialization::{
        serializer::serialize_account, solana_geyser_plugin_interface_shims::ReplicaAccountInfoV2,
    },
    serde_json::json,
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_sdk::{account::Account, commitment_config::CommitmentConfig, pubkey::Pubkey},
    std::str::FromStr,
};

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
        account: String,
    },
    Mint {
        // puts in mint, token, and metadata account
        #[arg(long)]
        mint: String,
    },
    Scenario {
        #[arg(long)]
        scenario_file: String,
    },
}
const STREAM: &str = "ACC";

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let config_wrapper = Value::from(map! {
    "redis_connection_str" => cli.redis_url,
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
    messenger.add_stream(STREAM).await.unwrap();
    messenger.set_buffer_size(STREAM, 10000000000000000).await;

    let client = RpcClient::new(cli.rpc_url.clone());

    let cmd = cli.action;

    match cmd {
        Action::Single { account } => send_account(&account, &client, &mut messenger).await,
        Action::Mint { mint } => {
            let mint_key = Pubkey::from_str(&mint).expect("Failed to parse mint as pubkey");
            let metadata_account = find_metadata_account(&mint_key).0.to_string();

            let token_account = get_token_account(&client.url(), &mint).await;
            let mint_accounts = vec![mint, metadata_account, token_account];
            for account in mint_accounts {
                send_account(&account, &client, &mut messenger).await;
            }
        }
        Action::Scenario { scenario_file } => {
            let scenario = std::fs::read_to_string(scenario_file).unwrap();
            let scenario: Vec<String> = scenario.lines().map(|s| s.to_string()).collect();
            for account in scenario {
                send_account(&account, &client, &mut messenger).await;
            }
        }
    }
}

// returns token account belonging to mint
pub async fn get_token_account(endpoint: &str, mint: &str) -> String {
    let client = reqwest::Client::new();
    let body = json!({
        "jsonrpc": "2.0",
        "id": "acc-forwarder",
        "method": "getTokenLargestAccounts",
        "params": [mint]
    });

    let result = client
        .post(endpoint)
        .json(&body)
        .send()
        .await
        .map_err(|err| {
            println!("Failed to call rpc for getTokenLargestAccounts, {}", err);
        })
        .unwrap();

    let result = result
        .json::<serde_json::Value>()
        .await
        .map_err(|err| {
            println!("Failed to parse json for getTokenLargestAccounts, {}", err);
        })
        .unwrap();
    result["result"]["value"][0]["address"]
        .as_str()
        .unwrap_or("")
        .to_string()
}
pub async fn send_account(
    account: &str,
    client: &RpcClient,
    messenger: &mut Box<dyn plerkle_messenger::Messenger>,
) {
    let account_key = Pubkey::from_str(account).expect("Failed to parse mint as pubkey");
    let get_account_response = client
        .get_account_with_commitment(&account_key, CommitmentConfig::confirmed())
        .await
        .expect("Failed to get account");
    let account_data = get_account_response
        .value
        .unwrap_or_else(|| panic!("Account {} not found", account));
    let slot = get_account_response.context.slot;
    send(account_key, account_data, slot, messenger).await
}

pub async fn send(
    pubkey: Pubkey,
    account_data: Account,
    slot: u64,
    messenger: &mut Box<dyn plerkle_messenger::Messenger>,
) {
    let fbb = flatbuffers::FlatBufferBuilder::new();

    let account_info = ReplicaAccountInfoV2 {
        pubkey: &pubkey.to_bytes(),
        lamports: account_data.lamports,
        owner: &account_data.owner.to_bytes(),
        executable: account_data.executable,
        rent_epoch: account_data.rent_epoch,
        data: &account_data.data,
        write_version: 0,
        txn_signature: None,
    };
    let is_startup = false;

    let fbb = serialize_account(fbb, &account_info, slot, is_startup);
    let bytes = fbb.finished_data();

    messenger.send(STREAM, bytes).await.unwrap();
    println!("Sent account {} to stream", pubkey);
}
