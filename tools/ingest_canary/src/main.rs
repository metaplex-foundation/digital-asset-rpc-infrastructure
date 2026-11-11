//! End-to-end ingestion probe for a disposable validator with a faucet.
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::Parser;
use mpl_core::instructions::CreateV2Builder;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use tokio::time::{sleep, timeout};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    rpc_url: String,
    #[arg(long)]
    das_url: String,
    #[arg(long, default_value_t = 90)]
    timeout_seconds: u64,
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Deserialize)]
struct Asset {
    id: String,
    interface: String,
    ownership: Ownership,
}

#[derive(Deserialize)]
struct Ownership {
    owner: String,
}

impl Asset {
    fn matches(&self, asset: &str, owner: &str) -> bool {
        self.id == asset && self.interface == "MplCoreAsset" && self.ownership.owner == owner
    }
}

#[derive(Deserialize)]
struct AssetList {
    items: Vec<Asset>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    asset: String,
    owner: String,
    signature: String,
    slot: u64,
    duration_ms: u128,
}

async fn das_rpc<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<T> {
    let reply: RpcResponse<T> = client
        .post(url)
        .json(&serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if let Some(error) = reply.error {
        bail!("{method}: RPC error {}: {}", error.code, error.message);
    }
    reply.result.context("DAS response has no result")
}

async fn run(args: &Args) -> Result<Evidence> {
    let started = Instant::now();
    let rpc = RpcClient::new_with_timeout_and_commitment(
        args.rpc_url.clone(),
        Duration::from_secs(5),
        CommitmentConfig::confirmed(),
    );
    // These keys exist only in memory. A unique owner makes old indexed assets
    // incapable of satisfying this check. Never point this tool at mainnet.
    let owner = Keypair::new();
    let asset = Keypair::new();
    let airdrop = rpc
        .request_airdrop(&owner.pubkey(), 100_000_000)
        .await
        .context("funding canary owner from validator faucet")?;
    while !rpc.confirm_transaction(&airdrop).await? {
        sleep(Duration::from_millis(250)).await;
    }
    let instruction = CreateV2Builder::new()
        .asset(asset.pubkey())
        .payer(owner.pubkey())
        .owner(Some(owner.pubkey()))
        .name("DAS ingestion canary".to_string())
        // No off-chain download is needed to prove on-chain ingestion.
        .uri(String::new())
        .instruction();
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&owner.pubkey()),
        &[&owner, &asset],
        rpc.get_latest_blockhash().await?,
    );
    let signature = rpc
        .send_and_confirm_transaction(&transaction)
        .await
        .context("minting MPL Core canary")?;
    let account = rpc.get_account(&asset.pubkey()).await?;
    if account.owner != mpl_core::ID {
        bail!("minted account is not owned by MPL Core");
    }
    let slot = rpc.get_slot().await?;
    let asset = asset.pubkey().to_string();
    let owner = owner.pubkey().to_string();
    eprintln!("Minted canary {asset}; waiting for getAsset and getAssetsByOwner");
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let mut last_failure = String::new();
    loop {
        let check = async {
            let indexed: Asset = das_rpc(
                &http,
                &args.das_url,
                "getAsset",
                serde_json::json!({"id": asset}),
            )
            .await?;
            if !indexed.matches(&asset, &owner) {
                bail!("getAsset returned the wrong asset, interface or owner");
            }
            let owned: AssetList = das_rpc(
                &http,
                &args.das_url,
                "getAssetsByOwner",
                serde_json::json!({
                    "ownerAddress": owner, "page": 1, "limit": 10,
                }),
            )
            .await?;
            if !owned.items.iter().any(|item| item.matches(&asset, &owner)) {
                bail!("getAssetsByOwner has not indexed the canary");
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        match check {
            Ok(()) => {
                return Ok(Evidence {
                    asset,
                    owner,
                    signature: signature.to_string(),
                    slot,
                    duration_ms: started.elapsed().as_millis(),
                })
            }
            Err(error) => {
                let failure = error.to_string();
                if failure != last_failure {
                    eprintln!("Waiting for DAS ingestion: {failure}");
                    last_failure = failure;
                }
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let evidence = timeout(Duration::from_secs(args.timeout_seconds), run(&args))
        .await
        .context("DAS ingestion canary deadline exceeded")??;
    println!("{}", serde_json::to_string(&evidence)?);
    Ok(())
}
