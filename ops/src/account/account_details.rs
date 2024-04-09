use anyhow::Result;
use das_core::Rpc;
use solana_sdk::{account::Account, pubkey::Pubkey};

pub struct AccountDetails<'a> {
    pub account: Account,
    pub slot: u64,
    pub pubkey: &'a Pubkey,
}

impl<'a> AccountDetails<'a> {
    pub fn new(account: Account, slot: u64, pubkey: &'a Pubkey) -> Self {
        Self {
            account,
            slot,
            pubkey,
        }
    }

    pub async fn fetch(rpc: &Rpc, pubkey: &'a Pubkey) -> Result<Self> {
        let account_response = rpc.get_account(pubkey).await?;
        let slot = account_response.context.slot;

        let account = account_response
            .value
            .ok_or_else(|| anyhow::anyhow!("Account not found for pubkey: {}", pubkey))?;

        Ok(Self::new(account, slot, pubkey))
    }
}
