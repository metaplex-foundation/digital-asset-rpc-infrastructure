use super::BubblegumContext;
use crate::backfill::worker::ProofRepairArgs;
use crate::error::ErrorKind;
use crate::tree::TreeResponse;
use anyhow::{anyhow, Result};
use digital_asset_types::dapi::get_proof_for_asset;
use digital_asset_types::rpc::AssetProof;
use futures::stream::{FuturesUnordered, StreamExt};
use mpl_bubblegum::accounts::TreeConfig;
use sea_orm::SqlxPostgresConnector;
use sha3::{Digest, Keccak256};
use solana_sdk::pubkey::Pubkey;
use spl_account_compression::concurrent_tree_wrapper::ProveLeafArgs;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

trait TryFromAssetProof {
    fn try_from_asset_proof(proof: AssetProof) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

impl TryFromAssetProof for ProveLeafArgs {
    fn try_from_asset_proof(proof: AssetProof) -> Result<Self, anyhow::Error> {
        Ok(ProveLeafArgs {
            current_root: bs58::decode(&proof.root)
                .into_vec()
                .map_err(|e| anyhow!(e))?
                .try_into()
                .map_err(|_| anyhow!("Invalid root length"))?,
            leaf: bs58::decode(&proof.leaf)
                .into_vec()
                .map_err(|e| anyhow!(e))?
                .try_into()
                .map_err(|_| anyhow!("Invalid leaf length"))?,
            proof_vec: proof
                .proof
                .iter()
                .map(|p| {
                    bs58::decode(p)
                        .into_vec()
                        .map_err(|e| anyhow!(e))
                        .and_then(|v| v.try_into().map_err(|_| anyhow!("Invalid proof length")))
                })
                .collect::<Result<Vec<[u8; 32]>>>()?,
            index: proof.node_index as u32,
        })
    }
}

fn hash(left: &[u8], right: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(left);
    hasher.update(right);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

fn verify_merkle_proof(proof: &ProveLeafArgs) -> bool {
    let mut node = proof.leaf;
    for (i, sibling) in proof.proof_vec.iter().enumerate() {
        if (proof.index >> i) & 1 == 0 {
            node = hash(&node, sibling);
        } else {
            node = hash(sibling, &node);
        }
    }
    node == proof.current_root
}

pub fn leaf_proof_result(proof: AssetProof) -> Result<ProofResult, anyhow::Error> {
    match ProveLeafArgs::try_from_asset_proof(proof) {
        Ok(proof) if verify_merkle_proof(&proof) => Ok(ProofResult::Correct),
        Ok(_) => Ok(ProofResult::Incorrect),
        Err(_) => Ok(ProofResult::Corrupt),
    }
}

#[derive(Debug, Default)]
pub struct ProofReport {
    pub tree_pubkey: Pubkey,
    pub total_leaves: usize,
    pub incorrect_proofs: usize,
    pub not_found_proofs: usize,
    pub correct_proofs: usize,
    pub corrupt_proofs: usize,
}

#[derive(Debug)]
pub enum ProofResult {
    Correct,
    Incorrect,
    NotFound,
    Corrupt,
}

impl fmt::Display for ProofResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProofResult::Correct => write!(f, "Correct proof found"),
            ProofResult::Incorrect => write!(f, "Incorrect proof found"),
            ProofResult::NotFound => write!(f, "Proof not found"),
            ProofResult::Corrupt => write!(f, "Corrupt proof found"),
        }
    }
}

pub async fn check(
    context: BubblegumContext,
    tree: TreeResponse,
    max_concurrency: usize,
    proof_repair_worker: ProofRepairArgs,
) -> Result<ProofReport> {
    let (tree_config_pubkey, _) = TreeConfig::find_pda(&tree.pubkey);

    let pool = context.database_pool.clone();

    let account = context.solana_rpc.get_account(&tree_config_pubkey).await?;
    let account = account
        .value
        .ok_or_else(|| ErrorKind::Generic("Account not found".to_string()))?;

    let tree_config = TreeConfig::from_bytes(account.data.as_slice())?;

    let report = Arc::new(Mutex::new(ProofReport {
        tree_pubkey: tree.pubkey,
        total_leaves: tree_config.num_minted as usize,
        ..ProofReport::default()
    }));

    let mut tasks = FuturesUnordered::new();

    let (metadata_json_download_worker, proof_repair_worker) =
        proof_repair_worker.start(context, tree.pubkey).await?;

    for i in 0..tree_config.num_minted {
        if tasks.len() >= max_concurrency {
            tasks.next().await;
        }

        let db = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());
        let tree_pubkey = tree.pubkey;
        let report = Arc::clone(&report);
        let proof_repair_worker = proof_repair_worker.clone();

        tasks.push(tokio::spawn(async move {
            let (asset, _) = Pubkey::find_program_address(
                &[b"asset", &tree_pubkey.to_bytes(), &i.to_le_bytes()],
                &mpl_bubblegum::ID,
            );
            let proof_lookup: Result<ProofResult, anyhow::Error> =
                get_proof_for_asset(&db, asset.to_bytes().to_vec())
                    .await
                    .map_or_else(|_| Ok(ProofResult::NotFound), leaf_proof_result);

            let proof_lookup = proof_repair_worker.try_repair(proof_lookup, i, asset).await;

            if let Ok(proof_result) = proof_lookup {
                let mut report = report.lock().await;

                match proof_result {
                    ProofResult::Correct => report.correct_proofs += 1,
                    ProofResult::Incorrect => report.incorrect_proofs += 1,
                    ProofResult::NotFound => report.not_found_proofs += 1,
                    ProofResult::Corrupt => report.corrupt_proofs += 1,
                }

                debug!(
                    tree = %tree_pubkey,
                    leaf_index = i,
                    asset = %asset,
                    result = ?proof_result,
                    "Proof result for asset"
                );
            }
        }));
    }

    while tasks.next().await.is_some() {}

    drop(proof_repair_worker);

    if let Some(metadata_json_download_worker) = metadata_json_download_worker {
        if let Err(e) = metadata_json_download_worker.await {
            tracing::error!("Failed metadata_json_download_worker: {:?}", e);
        }
    }

    let final_report = Arc::try_unwrap(report)
        .expect("Failed to unwrap Arc")
        .into_inner();

    Ok(final_report)
}
