use {
    borsh::{BorshDeserialize, BorshSerialize},
    clap::Parser,
    solana_account_decoder::UiAccountEncoding,
    solana_client::{
        nonblocking::rpc_client::RpcClient,
        rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
        rpc_filter::{Memcmp, RpcFilterType},
        rpc_request::MAX_MULTIPLE_ACCOUNTS,
    },
    solana_sdk::{
        account::Account,
        pubkey::{Pubkey, PUBKEY_BYTES},
    },
    spl_account_compression::state::{ConcurrentMerkleTreeHeader, ConcurrentMerkleTreeHeaderData},
};

#[derive(Debug, Parser)]
struct Args {
    // Solana RPC endpoint
    #[arg(long, short)]
    rpc: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let client = RpcClient::new(args.rpc);

    // Initialized SPL Account Compression accounts
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
    println!("Received {} accounts", accounts.len());

    // Trying to extract authority pubkey
    let accounts = accounts
        .into_iter()
        .filter_map(|(pubkey, account)| {
            get_authority(&account.data)
                .ok()
                .map(|authority| (pubkey, authority))
        })
        .collect::<Vec<_>>();
    println!("Successfully parsed {} accounts", accounts.len());

    // Print only accounts where authority owner is bubblegum
    let mut id = 1;
    for accounts in accounts.chunks(MAX_MULTIPLE_ACCOUNTS) {
        let pubkeys = accounts
            .iter()
            .map(|(_pubkey, authority)| *authority)
            .collect::<Vec<_>>();
        let authority_accounts = client.get_multiple_accounts(pubkeys.as_slice()).await?;
        for (authority_account, (pubkey, _authority)) in authority_accounts.iter().zip(accounts) {
            if let Some(account) = authority_account {
                if account.owner == mpl_bubblegum::ID {
                    println!("{} {}", id, pubkey);
                    id += 1;
                }
            }
        }
    }

    Ok(())
}

fn get_authority(mut data: &[u8]) -> anyhow::Result<Pubkey> {
    // additional checks
    let header = ConcurrentMerkleTreeHeader::deserialize(&mut data)?;
    let ConcurrentMerkleTreeHeaderData::V1(header) = header.header;
    let data = header.try_to_vec()?;

    let offset = 4 + 4;
    Pubkey::try_from(&data[offset..offset + PUBKEY_BYTES]).map_err(Into::into)
}
