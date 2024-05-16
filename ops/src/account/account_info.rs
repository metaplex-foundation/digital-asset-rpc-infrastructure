use anyhow::Result;
use das_core::Rpc;
use program_transformers::AccountInfo;
use solana_sdk::pubkey::Pubkey;

#[derive(thiserror::Error, Debug)]
pub enum AccountInfoError {
    #[error("account not found for pubkey: {pubkey}")]
    NotFound { pubkey: Pubkey },
    #[error("failed to fetch account info")]
    SolanaRequestError(#[from] solana_client::client_error::ClientError),
}

pub async fn fetch(rpc: &Rpc, pubkey: Pubkey) -> Result<AccountInfo, AccountInfoError> {
    let account_response = rpc.get_account(&pubkey).await?;
    let slot = account_response.context.slot;

    let account = account_response
        .value
        .ok_or_else(|| AccountInfoError::NotFound { pubkey })?;

    Ok(AccountInfo {
        slot,
        pubkey,
        owner: account.owner,
        data: account.data,
    })
}
