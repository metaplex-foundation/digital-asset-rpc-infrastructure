use std::ops::Deref;
use std::{sync::Arc, time::Duration};
use std::collections::HashMap;
use anchor_lang::AnchorSerialize;

use async_trait::async_trait;
use serde_derive::{Deserialize, Serialize};
use serde_json::value::RawValue;
use solana_sdk::keccak;
use solana_sdk::keccak::Hash;
use solana_sdk::pubkey::Pubkey;
use tracing::{error, info};
use tokio::{sync::broadcast::Receiver, task::JoinError, time::Instant};
use blockbuster::programs::bubblegum::BubblegumInstruction;
use mpl_bubblegum::types::{LeafSchema, MetadataArgs};
use mpl_bubblegum::utils::get_asset_id;
use crate::error::RollupValidationError;

pub const MAX_ROLLUP_DOWNLOAD_ATTEMPTS: u8 = 5;


#[derive(Serialize, Deserialize, Clone)]
pub struct Rollup {
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    pub tree_id: Pubkey,
    pub rolled_mints: Vec<RolledMintInstruction>,
    pub raw_metadata_map: HashMap<String, Box<RawValue>>, // map by uri
    pub max_depth: u32,
    pub max_buffer_size: u32,

    // derived data
    pub merkle_root: [u8; 32],    // validate
    pub last_leaf_hash: [u8; 32], // validate
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RolledMintInstruction {
    pub tree_update: ChangeLogEventV1, // validate // derive from nonce
    pub leaf_update: LeafSchema,       // validate
    pub mint_args: MetadataArgs,
    // V0.1: enforce collection.verify == false
    // V0.1: enforce creator.verify == false
    // V0.2: add pub collection_signature: Option<Signature> - sign asset_id with collection authority
    // V0.2: add pub creator_signature: Option<Map<Pubkey, Signature>> - sign asset_id with creator authority to ensure verified creator
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    pub authority: Pubkey,
}

#[derive(Default, Clone)]
pub struct BatchMintInstruction {
    pub max_depth: u32,
    pub max_buffer_size: u32,
    pub num_minted: u64,
    pub root: [u8; 32],
    pub leaf: [u8; 32],
    pub index: u32,
    pub metadata_url: String,
    pub file_checksum: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChangeLogEventV1 {
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    pub id: Pubkey,
    pub path: Vec<PathNode>,
    pub seq: u64,
    pub index: u32,
}

#[derive(Serialize, Deserialize, PartialEq, Copy, Clone)]
pub struct PathNode {
    pub node: [u8; 32],
    pub index: u32,
}

impl From<&PathNode> for spl_account_compression::state::PathNode {
    fn from(value: &PathNode) -> Self {
        Self {
            node: value.node,
            index: value.index,
        }
    }
}
impl From<spl_account_compression::state::PathNode> for PathNode {
    fn from(value: spl_account_compression::state::PathNode) -> Self {
        Self {
            node: value.node,
            index: value.index,
        }
    }
}
impl From<&ChangeLogEventV1> for blockbuster::programs::bubblegum::ChangeLogEventV1 {
    fn from(value: &ChangeLogEventV1) -> Self {
        Self {
            id: value.id,
            path: value.path.iter().map(Into::into).collect::<Vec<_>>(),
            seq: value.seq,
            index: value.index,
        }
    }
}
impl From<blockbuster::programs::bubblegum::ChangeLogEventV1> for ChangeLogEventV1 {
    fn from(value: blockbuster::programs::bubblegum::ChangeLogEventV1) -> Self {
        Self {
            id: value.id,
            path: value.path.into_iter().map(Into::into).collect::<Vec<_>>(),
            seq: value.seq,
            index: value.index,
        }
    }
}

impl From<&RolledMintInstruction> for BubblegumInstruction {
    fn from(value: &RolledMintInstruction) -> Self {
        let hash = value.leaf_update.hash();
        Self {
            instruction: InstructionName::MintV1,
            tree_update: Some((&value.tree_update).into()),
            leaf_update: Some(LeafSchemaEvent::new(
                Version::V1,
                value.leaf_update.clone(),
                hash,
            )),
            payload: Some(Payload::MintV1 {
                args: value.mint_args.clone(),
                authority: value.authority,
                tree_id: value.tree_update.id,
            }),
        }
    }
}

pub struct RollupPersister<D: RollupDownloader> {
    rocks_client: Arc<rocks_db::Storage>,
    downloader: D,
    metrics: Arc<RollupPersisterMetricsConfig>,
}

pub struct RollupDownloaderForPersister {}

#[async_trait]
impl RollupDownloader for RollupDownloaderForPersister {
    async fn download_rollup(&self, url: &str) -> Result<Box<Rollup>, UsecaseError> {
        let response = reqwest::get(url).await?.bytes().await?;
        Ok(Box::new(serde_json::from_slice(&response)?))
    }

    async fn download_rollup_and_check_checksum(
        &self,
        url: &str,
        checksum: &str,
    ) -> Result<Box<Rollup>, UsecaseError> {
        let response = reqwest::get(url).await?.bytes().await?;
        let file_hash = xxhash_rust::xxh3::xxh3_128(&response);
        let hash_hex = hex::encode(file_hash.to_be_bytes());
        if hash_hex != checksum {
            return Err(UsecaseError::HashMismatch(checksum.to_string(), hash_hex));
        }
        Ok(Box::new(serde_json::from_slice(&response)?))
    }
}

impl<D: RollupDownloader> RollupPersister<D> {
    pub fn new(
        rocks_client: Arc<rocks_db::Storage>,
        downloader: D,
        metrics: Arc<RollupPersisterMetricsConfig>,
    ) -> Self {
        Self {
            rocks_client,
            downloader,
            metrics,
        }
    }

    pub async fn persist_rollups(&self, mut rx: Receiver<()>) -> Result<(), JoinError> {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                        let (rollup_to_verify, rollup) = match self.get_rollup_to_verify().await {
                            Ok(res) => res,
                            Err(_) => {
                                continue;
                            }
                        };
                        let Some(rollup_to_verify) = rollup_to_verify else {
                            // no rollups to persist
                            return Ok(());
                        };
                        self.persist_rollup(&rx, rollup_to_verify, rollup).await
                    },
                _ = rx.recv() => {
                    info!("Received stop signal, stopping ...");
                    return Ok(());
                },
            }
        }
    }

    pub async fn persist_rollup(
        &self,
        rx: &Receiver<()>,
        mut rollup_to_verify: RollupToVerify,
        mut rollup: Option<Box<Rollup>>,
    ) {
        let start_time = Instant::now();
        info!("Persisting {} rollup", &rollup_to_verify.url);
        while rx.is_empty() {
            match (&rollup_to_verify.persisting_state, &rollup) {
                (&PersistingRollupState::ReceivedTransaction, _) | (_, None) => {
                    if let Err(err) = self
                        .download_rollup(&mut rollup_to_verify, &mut rollup)
                        .await
                    {
                        error!("Error during rollup downloading: {}", err)
                    };
                }
                (&PersistingRollupState::SuccessfullyDownload, Some(r)) => {
                    self.validate_rollup(&mut rollup_to_verify, r).await;
                }
                (&PersistingRollupState::SuccessfullyValidate, Some(r)) => {
                    self.store_rollup_update(&mut rollup_to_verify, r).await;
                }
                (&PersistingRollupState::FailedToPersist, _)
                | (&PersistingRollupState::StoredUpdate, _) => {
                    self.drop_rollup_from_queue(rollup_to_verify.file_hash.clone())
                        .await;
                    info!(
                        "Finish processing {} rollup file with {:?} state",
                        &rollup_to_verify.url, &rollup_to_verify.persisting_state
                    );
                    self.metrics.set_persisting_latency(
                        "rollup_persisting",
                        start_time.elapsed().as_millis() as f64,
                    );
                    return;
                }
            }
        }
    }

    async fn get_rollup_to_verify(
        &self,
    ) -> Result<(Option<RollupToVerify>, Option<Box<Rollup>>), IngesterError> {
        match self.rocks_client.fetch_rollup_for_verifying().await {
            Ok((Some(rollup_to_verify), rollup_data)) => {
                Ok((Some(rollup_to_verify), rollup_data.map(Box::new)))
            }
            Ok((None, _)) => Ok((None, None)),
            Err(e) => {
                self.metrics
                    .inc_rollups_with_status("rollup_fetch", MetricStatus::FAILURE);
                error!("Failed to fetch rollup for verifying: {}", e);
                Err(e.into())
            }
        }
    }

    async fn download_rollup(
        &self,
        rollup_to_verify: &mut RollupToVerify,
        rollup: &mut Option<Box<Rollup>>,
    ) -> Result<(), IngesterError> {
        if rollup.is_some() {
            return Ok(());
        }
        match self
            .downloader
            .download_rollup_and_check_checksum(
                rollup_to_verify.url.as_ref(),
                &rollup_to_verify.file_hash,
            )
            .await
        {
            Ok(r) => {
                self.metrics
                    .inc_rollups_with_status("rollup_download", MetricStatus::SUCCESS);
                if let Err(e) = self
                    .rocks_client
                    .rollups
                    .put(rollup_to_verify.file_hash.clone(), r.deref().clone())
                {
                    self.metrics
                        .inc_rollups_with_status("persist_rollup", MetricStatus::FAILURE);
                    return Err(e.into());
                }
                *rollup = Some(r);
                rollup_to_verify.persisting_state = PersistingRollupState::SuccessfullyDownload;
            }
            Err(e) => {
                if let UsecaseError::HashMismatch(expected, actual) = e {
                    rollup_to_verify.persisting_state = PersistingRollupState::FailedToPersist;
                    self.metrics
                        .inc_rollups_with_status("rollup_checksum_verify", MetricStatus::FAILURE);
                    self.save_rollup_as_failed(
                        FailedRollupState::ChecksumVerifyFailed,
                        rollup_to_verify,
                    )?;

                    return Err(IngesterError::RollupValidation(
                        RollupValidationError::FileChecksumMismatch(expected, actual),
                    ));
                }
                if let UsecaseError::Serialization(e) = e {
                    rollup_to_verify.persisting_state = PersistingRollupState::FailedToPersist;
                    self.metrics.inc_rollups_with_status(
                        "rollup_file_deserialization",
                        MetricStatus::FAILURE,
                    );
                    self.save_rollup_as_failed(
                        FailedRollupState::FileSerialization,
                        rollup_to_verify,
                    )?;

                    return Err(IngesterError::SerializatonError(e));
                }

                self.metrics
                    .inc_rollups_with_status("rollup_download", MetricStatus::FAILURE);
                if rollup_to_verify.download_attempts + 1 > MAX_ROLLUP_DOWNLOAD_ATTEMPTS {
                    rollup_to_verify.persisting_state = PersistingRollupState::FailedToPersist;
                    self.save_rollup_as_failed(
                        FailedRollupState::DownloadFailed,
                        rollup_to_verify,
                    )?;
                } else {
                    rollup_to_verify.download_attempts = rollup_to_verify.download_attempts + 1;
                    if let Err(e) = self.rocks_client.rollup_to_verify.put(
                        rollup_to_verify.file_hash.clone(),
                        RollupToVerify {
                            file_hash: rollup_to_verify.file_hash.clone(),
                            url: rollup_to_verify.url.clone(),
                            created_at_slot: rollup_to_verify.created_at_slot,
                            signature: rollup_to_verify.signature,
                            download_attempts: rollup_to_verify.download_attempts + 1,
                            persisting_state: PersistingRollupState::FailedToPersist,
                        },
                    ) {
                        self.metrics.inc_rollups_with_status(
                            "rollup_attempts_update",
                            MetricStatus::FAILURE,
                        );
                        return Err(e.into());
                    }
                    self.metrics
                        .inc_rollups_with_status("rollup_attempts_update", MetricStatus::SUCCESS);
                }
                return Err(IngesterError::Usecase(e.to_string()));
            }
        }
        Ok(())
    }

    async fn validate_rollup(&self, rollup_to_verify: &mut RollupToVerify, rollup: &Rollup) {
        if let Err(e) = crate::rollup::rollup_verifier::validate_rollup(rollup).await {
            self.metrics
                .inc_rollups_with_status("rollup_validating", MetricStatus::FAILURE);
            error!("Error while validating rollup: {}", e.to_string());

            rollup_to_verify.persisting_state = PersistingRollupState::FailedToPersist;
            if let Err(err) =
                self.save_rollup_as_failed(FailedRollupState::RollupVerifyFailed, rollup_to_verify)
            {
                error!("Save rollup as failed: {}", err);
            };
            return;
        }
        self.metrics
            .inc_rollups_with_status("rollup_validating", MetricStatus::SUCCESS);
        rollup_to_verify.persisting_state = PersistingRollupState::SuccessfullyValidate;
    }

    async fn store_rollup_update(&self, rollup_to_verify: &mut RollupToVerify, rollup: &Rollup) {
        if BubblegumTxProcessor::store_rollup_update(
            rollup_to_verify.created_at_slot,
            rollup,
            self.rocks_client.clone(),
            rollup_to_verify.signature,
        )
            .await
            .is_err()
        {
            self.metrics
                .inc_rollups_with_status("rollup_persist", MetricStatus::FAILURE);
            rollup_to_verify.persisting_state = PersistingRollupState::FailedToPersist;
            return;
        }
        self.metrics
            .inc_rollups_with_status("rollup_persist", MetricStatus::SUCCESS);
        rollup_to_verify.persisting_state = PersistingRollupState::StoredUpdate;
    }

    fn save_rollup_as_failed(
        &self,
        status: FailedRollupState,
        rollup: &RollupToVerify,
    ) -> Result<(), IngesterError> {
        let key = FailedRollupKey {
            status: status.clone(),
            hash: rollup.file_hash.clone(),
        };
        let value = FailedRollup {
            status,
            file_hash: rollup.file_hash.clone(),
            url: rollup.url.clone(),
            created_at_slot: rollup.created_at_slot,
            signature: rollup.signature,
            download_attempts: rollup.download_attempts + 1,
        };
        if let Err(e) = self.rocks_client.failed_rollups.put(key, value) {
            self.metrics
                .inc_rollups_with_status("rollup_mark_as_failure", MetricStatus::FAILURE);
            return Err(e.into());
        }
        Ok(())
    }

    async fn drop_rollup_from_queue(&self, file_hash: String) {
        if let Err(e) = self.rocks_client.drop_rollup_from_queue(file_hash).await {
            self.metrics
                .inc_rollups_with_status("rollup_queue_clear", MetricStatus::FAILURE);
            error!("Rollup queue clear: {}", e)
        }
    }
}

async fn validate_rollup(rollup: &Rollup) -> Result<(), RollupValidationError> {
    let mut leaf_hashes = Vec::new();
    for asset in rollup.rolled_mints.iter() {
        let leaf_hash = match get_leaf_hash(asset, &rollup.tree_id) {
            Ok(leaf_hash) => leaf_hash,
            Err(e) => {
                return Err(e);
            }
        };
        leaf_hashes.push(leaf_hash);
    }

    usecase::merkle_tree::validate_change_logs(
        rollup.max_depth,
        rollup.max_buffer_size,
        &leaf_hashes,
        rollup,
    )
}

fn get_leaf_hash(
    asset: &RolledMintInstruction,
    tree_id: &Pubkey,
) -> Result<[u8; 32], RollupValidationError> {
    let asset_id = get_asset_id(tree_id, asset.leaf_update.nonce());
    if asset_id != asset.leaf_update.id() {
        return Err(RollupValidationError::PDACheckFail(
            asset_id.to_string(),
            asset.leaf_update.id().to_string(),
        ));
    }

    // @dev: seller_fee_basis points is encoded twice so that it can be passed to marketplace
    // instructions, without passing the entire, un-hashed MetadataArgs struct
    let metadata_args_hash = keccak::hashv(&[asset.mint_args.try_to_vec()?.as_slice()]);
    let data_hash = keccak::hashv(&[
        &metadata_args_hash.to_bytes(),
        &asset.mint_args.seller_fee_basis_points.to_le_bytes(),
    ]);
    if asset.leaf_update.data_hash() != data_hash.to_bytes() {
        return Err(RollupValidationError::InvalidDataHash(
            data_hash.to_string(),
            Hash::new(asset.leaf_update.data_hash().as_slice()).to_string(),
        ));
    }

    // Use the metadata auth to check whether we can allow `verified` to be set to true in the
    // creator Vec.
    let creator_data = asset
        .mint_args
        .creators
        .iter()
        .map(|c| [c.address.as_ref(), &[c.verified as u8], &[c.share]].concat())
        .collect::<Vec<_>>();

    // Calculate creator hash.
    let creator_hash = keccak::hashv(
        creator_data
            .iter()
            .map(|c| c.as_slice())
            .collect::<Vec<&[u8]>>()
            .as_ref(),
    );
    if asset.leaf_update.creator_hash() != creator_hash.to_bytes() {
        return Err(RollupValidationError::InvalidCreatorsHash(
            creator_hash.to_string(),
            Hash::new(asset.leaf_update.creator_hash().as_slice()).to_string(),
        ));
    }

    Ok(asset.leaf_update.hash())
}
