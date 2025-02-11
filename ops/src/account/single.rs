use anyhow::Result;

use super::account_details::AccountDetails;
use clap::Parser;
use das_core::{MetricsArgs, QueueArgs, QueuePool, Rpc, SolanaRpcArgs};
use flatbuffers::FlatBufferBuilder;
use plerkle_serialization::{
    serializer::serialize_account, solana_geyser_plugin_interface_shims::ReplicaAccountInfoV2,
};
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Redis configuration
    #[clap(flatten)]
    pub queue: QueueArgs,

    /// Metrics configuration
    #[clap(flatten)]
    pub metrics: MetricsArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,
    /// The public key of the account to backfill
    #[clap(value_parser = parse_pubkey)]
    pub account: Pubkey,
}

fn parse_pubkey(s: &str) -> Result<Pubkey, &'static str> {
    Pubkey::try_from(s).map_err(|_| "Failed to parse public key")
}

pub async fn run(config: Args) -> Result<()> {
    let rpc = Rpc::from_config(config.solana);
    let queue = QueuePool::try_from_config(config.queue).await?;

    let AccountDetails {
        account,
        slot,
        pubkey,
    } = AccountDetails::fetch(&rpc, &config.account).await?;
    let builder = FlatBufferBuilder::new();
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

    let fbb = serialize_account(builder, &account_info, slot, false);
    let bytes = fbb.finished_data();

    queue.push_account_backfill(bytes).await?;

    Ok(())
}
