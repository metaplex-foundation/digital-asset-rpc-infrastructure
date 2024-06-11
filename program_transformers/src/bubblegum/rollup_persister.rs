use anchor_lang::AnchorSerialize;
use std::collections::HashMap;
use std::ops::Deref;
use std::{sync::Arc, time::Duration};

use crate::bubblegum;
use crate::error::{ProgramTransformerError, RollupValidationError};
use async_trait::async_trait;
use blockbuster::programs::bubblegum::{BubblegumInstruction, Payload};
use digital_asset_types::dao::prelude::RollupToVerify;
use digital_asset_types::dao::rollup_to_verify;
use digital_asset_types::dao::sea_orm_active_enums::{RollupFailStatus, RollupPersistingState};
use mockall::automock;
use mpl_bubblegum::types::{LeafSchema, MetadataArgs, Version};
use mpl_bubblegum::utils::get_asset_id;
use mpl_bubblegum::{InstructionName, LeafSchemaEvent};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, Condition, ConnectionTrait, EntityTrait};
use serde_derive::{Deserialize, Serialize};
use serde_json::value::RawValue;
use solana_sdk::keccak;
use solana_sdk::keccak::Hash;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use tokio::{sync::broadcast::Receiver, task::JoinError, time::Instant};
use tracing::{error, info};

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

#[automock]
#[async_trait]
pub trait RollupDownloader {
    async fn download_rollup(&self, url: &str) -> Result<Box<Rollup>, ProgramTransformerError>;
    async fn download_rollup_and_check_checksum(
        &self,
        url: &str,
        checksum: &str,
    ) -> Result<Box<Rollup>, ProgramTransformerError>;
}

pub struct RollupPersister<T: ConnectionTrait, D: RollupDownloader> {
    txn: Arc<T>,
    downloader: D,
    cl_audits: bool,
}

pub struct RollupDownloaderForPersister {}

#[async_trait]
impl RollupDownloader for RollupDownloaderForPersister {
    async fn download_rollup(&self, url: &str) -> Result<Box<Rollup>, RollupValidationError> {
        let response = reqwest::get(url).await?.bytes().await?;
        Ok(Box::new(serde_json::from_slice(&response)?))
    }

    async fn download_rollup_and_check_checksum(
        &self,
        url: &str,
        checksum: &str,
    ) -> Result<Box<Rollup>, RollupValidationError> {
        let response = reqwest::get(url).await?.bytes().await?;
        let file_hash = xxhash_rust::xxh3::xxh3_128(&response);
        let hash_hex = hex::encode(file_hash.to_be_bytes());
        if hash_hex != checksum {
            return Err(RollupValidationError::InvalidDataHash(
                checksum.to_string(),
                hash_hex,
            ));
        }
        Ok(Box::new(serde_json::from_slice(&response)?))
    }
}

impl<T: ConnectionTrait, D: RollupDownloader> RollupPersister<T, D> {
    pub fn new(txn: Arc<T>, downloader: D, cl_audits: bool) -> Self {
        Self {
            txn,
            downloader,
            cl_audits,
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
        mut rollup_to_verify: rollup_to_verify::Model,
        mut rollup: Option<Box<Rollup>>,
    ) {
        info!("Persisting {} rollup", &rollup_to_verify.url);
        while rx.is_empty() {
            match &rollup_to_verify.rollup_persisting_state {
                &RollupPersistingState::ReceivedTransaction => {
                    if let Err(err) = self
                        .download_rollup(&mut rollup_to_verify, &mut rollup)
                        .await
                    {
                        error!("Error during rollup downloading: {}", err)
                    };
                }
                &RollupPersistingState::SuccessfullyDownload => {
                    if let Some(r) = &rollup {
                        self.validate_rollup(&mut rollup_to_verify, r).await;
                    } else {
                        error!(
                            "Trying to validate non downloaded rollup: {:#?}",
                            &rollup_to_verify
                        )
                    }
                }
                &RollupPersistingState::SuccessfullyValidate => {
                    if let Some(r) = &rollup {
                        self.store_rollup_update(&mut rollup_to_verify, r).await;
                    } else {
                        error!(
                            "Trying to store update for non downloaded rollup: {:#?}",
                            &rollup_to_verify
                        )
                    }
                }
                &RollupPersistingState::FailedToPersist | &RollupPersistingState::StoredUpdate => {
                    self.drop_rollup_from_queue(rollup_to_verify.file_hash.clone())
                        .await;
                    info!(
                        "Finish processing {} rollup file with {:?} state",
                        &rollup_to_verify.url, &rollup_to_verify.rollup_persisting_state
                    );
                    return;
                }
            }
        }
    }

    async fn get_rollup_to_verify(
        &self,
    ) -> Result<(Option<rollup_to_verify::Model>, Option<Box<Rollup>>), ProgramTransformerError>
    {
        let condition = Condition::all()
            .add(
                rollup_to_verify::Column::RollupPersistingState
                    .ne(RollupPersistingState::FailedToPersist),
            )
            .add(
                rollup_to_verify::Column::RollupPersistingState
                    .ne(RollupPersistingState::StoredUpdate),
            );

        rollup_to_verify::Entity::find()
            .filter(condition)
            .order_by_asc(rollup_to_verify::Column::CreatedAtSlot)
            .one(self.txn.as_ref())
            .await
    }

    async fn download_rollup(
        &self,
        rollup_to_verify: &mut rollup_to_verify::Model,
        rollup: &mut Option<Box<Rollup>>,
    ) -> Result<(), ProgramTransformerError> {
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
                if let Err(e) = self
                    .rocks_client
                    .rollups
                    .put(rollup_to_verify.file_hash.clone(), r.deref().clone())
                {
                    return Err(e.into());
                }
                *rollup = Some(r);
                rollup_to_verify.rollup_persisting_state =
                    RollupPersistingState::SuccessfullyDownload;
            }
            Err(e) => {
                if let RollupValidationError::InvalidDataHash(expected, actual) = e {
                    rollup_to_verify.rollup_persisting_state =
                        RollupPersistingState::FailedToPersist;
                    self.save_rollup_as_failed(
                        RollupFailStatus::ChecksumVerifyFailed,
                        rollup_to_verify,
                    )
                    .await?;

                    return Err(ProgramTransformerError::RollupValidation(
                        RollupValidationError::FileChecksumMismatch(expected, actual),
                    ));
                }
                if let ProgramTransformerError::Serialization(e) = e {
                    rollup_to_verify.rollup_persisting_state =
                        RollupPersistingState::FailedToPersist;
                    self.save_rollup_as_failed(
                        RollupFailStatus::FileSerialization,
                        rollup_to_verify,
                    )?;

                    return Err(ProgramTransformerError::SerializatonError(e));
                }
                if rollup_to_verify.download_attempts + 1 > MAX_ROLLUP_DOWNLOAD_ATTEMPTS {
                    rollup_to_verify.rollup_persisting_state =
                        RollupPersistingState::FailedToPersist;
                    self.save_rollup_as_failed(RollupFailStatus::DownloadFailed, rollup_to_verify)?;
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
                            persisting_state: RollupPersistingState::FailedToPersist,
                        },
                    ) {
                        return Err(e.into());
                    }
                }
                return Err(ProgramTransformerError::Usecase(e.to_string()));
            }
        }
        Ok(())
    }

    async fn validate_rollup(
        &self,
        rollup_to_verify: &mut rollup_to_verify::Model,
        rollup: &Rollup,
    ) {
        if let Err(e) = validate_rollup(rollup).await {
            error!("Error while validating rollup: {}", e.to_string());

            rollup_to_verify.rollup_persisting_state = RollupPersistingState::FailedToPersist;
            if let Err(err) =
                self.save_rollup_as_failed(RollupFailStatus::RollupVerifyFailed, rollup_to_verify)
            {
                error!("Save rollup as failed: {}", err);
            };
            return;
        }
        rollup_to_verify.rollup_persisting_state = RollupPersistingState::SuccessfullyValidate;
    }

    async fn store_rollup_update(
        &self,
        rollup_to_verify: &mut rollup_to_verify::Model,
        rollup: &Rollup,
    ) {
        if bubblegum::finalize_tree_with_root::store_rollup_update(
            rollup_to_verify.created_at_slot as u64,
            Signature::try_from(rollup_to_verify.signature.clone())
                .map_err(|e| ProgramTransformerError::SerializatonError(format!("{:?}", e)))?,
            rollup,
            self.txn.as_ref(),
            self.cl_audits,
        )
        .await
        .is_err()
        {
            rollup_to_verify.rollup_persisting_state = RollupPersistingState::FailedToPersist;
            return;
        }
        rollup_to_verify.rollup_persisting_state = RollupPersistingState::StoredUpdate;
    }

    async fn save_rollup_as_failed(
        &self,
        status: RollupFailStatus,
        rollup: &rollup_to_verify::Model,
    ) -> Result<(), ProgramTransformerError> {
        rollup_to_verify::ActiveModel {
            file_hash: Set(rollup.file_hash.clone()),
            url: Set(rollup.url.clone()),
            created_at_slot: Set(rollup.created_at_slot),
            signature: Set(rollup.signature.clone()),
            download_attempts: Set(rollup.download_attempts),
            rollup_persisting_state: Set(rollup.rollup_persisting_state.clone()),
            rollup_fail_status: Set(Some(status)),
        }
        .insert(self.txn.as_ref())
        .await
        .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn drop_rollup_from_queue(&self, file_hash: String) {
        if let Some(existing_rollup) = rollup_to_verify::Entity::find_by_id(file_hash.clone())
            .one(self.txn.as_ref())
            .await?
        {
            let mut active_model: rollup_to_verify::ActiveModel = existing_rollup.into();
            active_model.rollup_persisting_state = Set(RollupPersistingState::StoredUpdate);
            active_model.update(self.txn.as_ref()).await?;
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

    bubblegum::merkle_tree_wrapper::validate_change_logs(
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
