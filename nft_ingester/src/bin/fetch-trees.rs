use {
    clap::Parser,
    solana_account_decoder::UiAccountEncoding,
    solana_client::{
        nonblocking::rpc_client::RpcClient,
        rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
        rpc_filter::{Memcmp, RpcFilterType},
    },
    solana_sdk::{account::Account, pubkey::Pubkey},
};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, short)]
    rpc: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let client = RpcClient::new(args.rpc);
    let config = RpcProgramAccountsConfig {
        filters: Some(vec![RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            0,
            vec![1u8],
        ))]),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            ..Default::default()
        },
        ..Default::default()
    };
    let accounts: Vec<(Pubkey, Account)> = client
        .get_program_accounts_with_config(&spl_account_compression::id(), config)
        .await?;

    println!("{:?}", accounts.len());
    for (i, (pubkey, _account)) in accounts.iter().enumerate() {
        println!("{} {:?}", i + 1, pubkey);
    }

    Ok(())
}
