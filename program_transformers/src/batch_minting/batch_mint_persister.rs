use anchor_lang::AnchorSerialize;
use async_channel::Receiver;
use std::collections::HashMap;
use std::sync::Arc;

use crate::batch_minting::merkle_tree_wrapper;
use crate::bubblegum;
use crate::error::{BatchMintValidationError, ProgramTransformerError, ProgramTransformerResult};
use async_trait::async_trait;
use blockbuster::instruction::InstructionBundle;
use blockbuster::programs::bubblegum::{BubblegumInstruction, Payload};
use cadence_macros::{statsd_count, statsd_histogram};
use digital_asset_types::dao::sea_orm_active_enums::{RollupFailStatus, RollupPersistingState};
use digital_asset_types::dao::{rollup, rollup_to_verify};
use mockall::automock;
use mpl_bubblegum::types::{LeafSchema, MetadataArgs, Version};
use mpl_bubblegum::utils::get_asset_id;
use mpl_bubblegum::{InstructionName, LeafSchemaEvent};
use sea_orm::sea_query::{LockType, OnConflict};
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbBackend, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, QuerySelect, QueryTrait, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use solana_sdk::keccak;
use solana_sdk::keccak::Hash;
use solana_sdk::pubkey::Pubkey;
use tokio::time::Instant;
use tracing::{error, info};

pub const MAX_BATCH_MINT_DOWNLOAD_ATTEMPTS: u8 = 5;

#[derive(Serialize, Deserialize, Clone)]
pub struct BatchMint {
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    pub tree_id: Pubkey,
    pub batch_mints: Vec<BatchedMintInstruction>,
    pub raw_metadata_map: HashMap<String, Box<RawValue>>, // map by uri
    pub max_depth: u32,
    pub max_buffer_size: u32,

    // derived data
    pub merkle_root: [u8; 32],
    pub last_leaf_hash: [u8; 32],
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BatchedMintInstruction {
    pub tree_update: ChangeLogEventV1,
    pub leaf_update: LeafSchema,
    pub mint_args: MetadataArgs,
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

impl From<&BatchedMintInstruction> for BubblegumInstruction {
    fn from(value: &BatchedMintInstruction) -> Self {
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
pub trait BatchMintDownloader {
    async fn download_batch_mint(
        &self,
        url: &str,
    ) -> Result<Box<BatchMint>, BatchMintValidationError>;
    async fn download_batch_mint_and_check_checksum(
        &self,
        url: &str,
        checksum: &str,
    ) -> Result<Box<BatchMint>, BatchMintValidationError>;
}

pub struct BatchMintPersister<T: ConnectionTrait + TransactionTrait, D: BatchMintDownloader> {
    txn: Arc<T>,
    notification_receiver: Receiver<()>,
    downloader: D,
}

pub struct BatchMintDownloaderForPersister {}

#[async_trait]
impl BatchMintDownloader for BatchMintDownloaderForPersister {
    async fn download_batch_mint(
        &self,
        url: &str,
    ) -> Result<Box<BatchMint>, BatchMintValidationError> {
        let response = reqwest::get(url).await?.bytes().await?;
        Ok(Box::new(serde_json::from_slice(&response)?))
    }

    async fn download_batch_mint_and_check_checksum(
        &self,
        url: &str,
        checksum: &str,
    ) -> Result<Box<BatchMint>, BatchMintValidationError> {
        let response = reqwest::get(url).await?.bytes().await?;
        let file_hash = xxhash_rust::xxh3::xxh3_128(&response);
        let hash_hex = hex::encode(file_hash.to_be_bytes());
        if hash_hex != checksum {
            return Err(BatchMintValidationError::InvalidDataHash(
                checksum.to_string(),
                hash_hex,
            ));
        }
        Ok(Box::new(serde_json::from_slice(&response)?))
    }
}

impl<T: ConnectionTrait + TransactionTrait, D: BatchMintDownloader> BatchMintPersister<T, D> {
    pub fn new(txn: Arc<T>, notification_receiver: Receiver<()>, downloader: D) -> Self {
        Self {
            txn,
            notification_receiver,
            downloader,
        }
    }

    pub async fn persist_batch_mints(&self) {
        loop {
            if let Err(e) = self.notification_receiver.recv().await {
                error!("Recv batch mint notification: {}", e);
                continue;
            };
            let Ok((batch_mint_to_verify, batch_mint)) = self.get_batch_mint_to_verify().await
            else {
                statsd_count!("batch_mint.fail_get_rollup", 1);
                continue;
            };
            let Some(batch_mint_to_verify) = batch_mint_to_verify else {
                // no batch mints to persist
                continue;
            };
            let batch_mint = batch_mint
                .map(|r| bincode::deserialize::<BatchMint>(r.rollup_binary_bincode.as_slice()))
                .transpose()
                .unwrap_or_default();
            self.persist_batch_mint(batch_mint_to_verify, batch_mint.map(Box::new))
                .await;
        }
    }

    pub async fn persist_batch_mint(
        &self,
        mut batch_mint_to_verify: rollup_to_verify::Model,
        mut batch_mint: Option<Box<BatchMint>>,
    ) {
        let start_time = Instant::now();
        info!("Persisting {} batch mint", &batch_mint_to_verify.url);
        loop {
            match &batch_mint_to_verify.rollup_persisting_state {
                &RollupPersistingState::ReceivedTransaction => {
                    // We get ReceivedTransaction state on the start of processing
                    batch_mint_to_verify.rollup_persisting_state =
                        RollupPersistingState::StartProcessing;
                }
                &RollupPersistingState::StartProcessing => {
                    if let Err(err) = self
                        .download_batch_mint(&mut batch_mint_to_verify, &mut batch_mint)
                        .await
                    {
                        error!("Error during batch mint downloading: {}", err)
                    };
                }
                &RollupPersistingState::SuccessfullyDownload => {
                    if let Some(r) = &batch_mint {
                        self.validate_batch_mint(&mut batch_mint_to_verify, r).await;
                    } else {
                        error!(
                            "Trying to validate non downloaded batch mint: {:#?}",
                            &batch_mint_to_verify
                        )
                    }
                }
                &RollupPersistingState::SuccessfullyValidate => {
                    if let Some(r) = &batch_mint {
                        if let Err(e) = self
                            .store_batch_mint_update(&mut batch_mint_to_verify, r)
                            .await
                        {
                            error!("Store batch mint update: {}", e)
                        };
                    } else {
                        error!(
                            "Trying to store update for non downloaded batch mint: {:#?}",
                            &batch_mint_to_verify
                        )
                    }
                }
                &RollupPersistingState::FailedToPersist | &RollupPersistingState::StoredUpdate => {
                    if let Err(e) = self.drop_batch_mint_from_queue(&batch_mint_to_verify).await {
                        error!("failed to drop batch mint from queue: {}", e);
                    };
                    info!(
                        "Finish processing {} batch mint file with {:?} state",
                        &batch_mint_to_verify.url, &batch_mint_to_verify.rollup_persisting_state
                    );
                    statsd_histogram!(
                        "batch_mint.persisting_latency",
                        start_time.elapsed().as_millis() as u64
                    );
                    statsd_count!("batch_mint.total_processed", 1);
                    return;
                }
            }
        }
    }

    pub async fn get_batch_mint_to_verify(
        &self,
    ) -> Result<(Option<rollup_to_verify::Model>, Option<rollup::Model>), ProgramTransformerError>
    {
        let multi_txn = self.txn.begin().await?;
        let condition = Condition::all()
            .add(
                rollup_to_verify::Column::RollupPersistingState
                    .ne(RollupPersistingState::FailedToPersist),
            )
            .add(
                rollup_to_verify::Column::RollupPersistingState
                    .ne(RollupPersistingState::StoredUpdate),
            )
            .add(
                rollup_to_verify::Column::RollupPersistingState
                    .ne(RollupPersistingState::StartProcessing),
            );

        let batch_mint_verify = rollup_to_verify::Entity::find()
            .filter(condition)
            .order_by_asc(rollup_to_verify::Column::CreatedAtSlot)
            .lock(LockType::Update)
            .one(&multi_txn)
            .await
            .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
        let mut batch_mint = None;
        if let Some(ref r) = batch_mint_verify {
            batch_mint = rollup::Entity::find()
                .filter(rollup::Column::FileHash.eq(r.file_hash.clone()))
                .one(self.txn.as_ref())
                .await
                .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
            rollup_to_verify::Entity::update(rollup_to_verify::ActiveModel {
                file_hash: Set(r.file_hash.clone()),
                url: Set(r.url.clone()),
                created_at_slot: Set(r.created_at_slot),
                signature: Set(r.signature.clone()),
                staker: Set(r.staker.clone()),
                download_attempts: Set(r.download_attempts),
                rollup_persisting_state: Set(RollupPersistingState::StartProcessing),
                rollup_fail_status: Set(r.rollup_fail_status.clone()),
            })
            .filter(rollup_to_verify::Column::FileHash.eq(r.file_hash.clone()))
            .exec(&multi_txn)
            .await
            .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
        }
        multi_txn.commit().await?;

        Ok((batch_mint_verify, batch_mint))
    }

    async fn download_batch_mint(
        &self,
        batch_mint_to_verify: &mut rollup_to_verify::Model,
        batch_mint: &mut Option<Box<BatchMint>>,
    ) -> Result<(), ProgramTransformerError> {
        if batch_mint.is_some() {
            return Ok(());
        }
        match self
            .downloader
            .download_batch_mint_and_check_checksum(
                batch_mint_to_verify.url.as_ref(),
                &batch_mint_to_verify.file_hash,
            )
            .await
        {
            Ok(r) => {
                let query = rollup::Entity::insert(rollup::ActiveModel {
                    file_hash: Set(batch_mint_to_verify.file_hash.clone()),
                    rollup_binary_bincode: Set(bincode::serialize(batch_mint)
                        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?),
                })
                .on_conflict(
                    OnConflict::columns([rollup::Column::FileHash])
                        .update_columns([rollup::Column::RollupBinaryBincode])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);
                if let Err(e) = self.txn.execute(query).await {
                    return Err(e.into());
                }
                *batch_mint = Some(r);
                batch_mint_to_verify.rollup_persisting_state =
                    RollupPersistingState::SuccessfullyDownload;
                statsd_count!("batch_mint.successfully_download", 1);
            }
            Err(e) => {
                statsd_count!("batch_mint.download_fail", 1);
                if let BatchMintValidationError::InvalidDataHash(expected, actual) = e {
                    batch_mint_to_verify.rollup_persisting_state =
                        RollupPersistingState::FailedToPersist;
                    self.save_batch_mint_as_failed(
                        RollupFailStatus::ChecksumVerifyFailed,
                        batch_mint_to_verify,
                    )
                    .await?;

                    statsd_count!("batch_mint.checksum_verify_fail", 1);
                    return Err(ProgramTransformerError::BatchMintValidation(
                        BatchMintValidationError::FileChecksumMismatch(expected, actual),
                    ));
                }
                if let BatchMintValidationError::Serialization(e) = e {
                    batch_mint_to_verify.rollup_persisting_state =
                        RollupPersistingState::FailedToPersist;
                    self.save_batch_mint_as_failed(
                        RollupFailStatus::FileSerialization,
                        batch_mint_to_verify,
                    )
                    .await?;

                    statsd_count!("batch_mint.file_deserialization_fail", 1);
                    return Err(ProgramTransformerError::SerializatonError(e));
                }
                if batch_mint_to_verify.download_attempts + 1
                    > MAX_BATCH_MINT_DOWNLOAD_ATTEMPTS as i32
                {
                    batch_mint_to_verify.rollup_persisting_state =
                        RollupPersistingState::FailedToPersist;
                    self.save_batch_mint_as_failed(
                        RollupFailStatus::DownloadFailed,
                        batch_mint_to_verify,
                    )
                    .await?;
                } else {
                    batch_mint_to_verify.download_attempts =
                        batch_mint_to_verify.download_attempts + 1;
                    if let Err(e) = (rollup_to_verify::ActiveModel {
                        file_hash: Set(batch_mint_to_verify.file_hash.clone()),
                        url: Set(batch_mint_to_verify.url.clone()),
                        created_at_slot: Set(batch_mint_to_verify.created_at_slot),
                        signature: Set(batch_mint_to_verify.signature.clone()),
                        staker: Set(batch_mint_to_verify.staker.clone()),
                        download_attempts: Set(batch_mint_to_verify.download_attempts + 1),
                        rollup_persisting_state: Set(batch_mint_to_verify
                            .rollup_persisting_state
                            .clone()),
                        rollup_fail_status: Set(batch_mint_to_verify.rollup_fail_status.clone()),
                    }
                    .insert(self.txn.as_ref()))
                    .await
                    {
                        return Err(e.into());
                    }
                }
                return Err(ProgramTransformerError::BatchMintValidation(e));
            }
        }
        Ok(())
    }

    async fn validate_batch_mint(
        &self,
        batch_mint_to_verify: &mut rollup_to_verify::Model,
        batch_mint: &BatchMint,
    ) {
        if let Err(e) = validate_batch_mint(batch_mint).await {
            error!("Error while validating batch mint: {}", e.to_string());

            statsd_count!("batch_mint.validating_fail", 1);
            batch_mint_to_verify.rollup_persisting_state = RollupPersistingState::FailedToPersist;
            if let Err(err) = self
                .save_batch_mint_as_failed(
                    RollupFailStatus::RollupVerifyFailed,
                    batch_mint_to_verify,
                )
                .await
            {
                error!("Save batch mint as failed: {}", err);
            };
            return;
        }
        statsd_count!("batch_mint.validating_success", 1);
        batch_mint_to_verify.rollup_persisting_state = RollupPersistingState::SuccessfullyValidate;
    }

    async fn store_batch_mint_update(
        &self,
        batch_mint_to_verify: &mut rollup_to_verify::Model,
        batch_mint: &BatchMint,
    ) -> Result<(), ProgramTransformerError> {
        if store_batch_mint_update(
            batch_mint_to_verify.created_at_slot as u64,
            batch_mint_to_verify.signature.clone(),
            batch_mint,
            self.txn.as_ref(),
        )
        .await
        .is_err()
        {
            statsd_count!("batch_mint.store_update_fail", 1);
            batch_mint_to_verify.rollup_persisting_state = RollupPersistingState::FailedToPersist;
            return Ok(());
        }
        statsd_count!("batch_mint.store_update_success", 1);
        batch_mint_to_verify.rollup_persisting_state = RollupPersistingState::StoredUpdate;
        Ok(())
    }

    async fn save_batch_mint_as_failed(
        &self,
        status: RollupFailStatus,
        batch_mint: &rollup_to_verify::Model,
    ) -> Result<(), ProgramTransformerError> {
        rollup_to_verify::Entity::update(rollup_to_verify::ActiveModel {
            file_hash: Set(batch_mint.file_hash.clone()),
            url: Set(batch_mint.url.clone()),
            created_at_slot: Set(batch_mint.created_at_slot),
            signature: Set(batch_mint.signature.clone()),
            staker: Set(batch_mint.staker.clone()),
            download_attempts: Set(batch_mint.download_attempts),
            rollup_persisting_state: Set(batch_mint.rollup_persisting_state.clone()),
            rollup_fail_status: Set(Some(status)),
        })
        .filter(rollup_to_verify::Column::FileHash.eq(batch_mint.file_hash.clone()))
        .exec(self.txn.as_ref())
        .await
        .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn drop_batch_mint_from_queue(
        &self,
        batch_mint_to_verify: &rollup_to_verify::Model,
    ) -> Result<(), ProgramTransformerError> {
        rollup_to_verify::Entity::update(rollup_to_verify::ActiveModel {
            rollup_persisting_state: Set(batch_mint_to_verify.rollup_persisting_state.clone()),
            ..batch_mint_to_verify.clone().into_active_model()
        })
        .filter(rollup_to_verify::Column::FileHash.eq(batch_mint_to_verify.file_hash.clone()))
        .exec(self.txn.as_ref())
        .await
        .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

pub async fn validate_batch_mint(batch_mint: &BatchMint) -> Result<(), BatchMintValidationError> {
    let mut leaf_hashes = Vec::new();
    for asset in batch_mint.batch_mints.iter() {
        let leaf_hash = match get_leaf_hash(asset, &batch_mint.tree_id) {
            Ok(leaf_hash) => leaf_hash,
            Err(e) => {
                return Err(e);
            }
        };
        leaf_hashes.push(leaf_hash);
    }

    merkle_tree_wrapper::validate_change_logs(
        batch_mint.max_depth,
        batch_mint.max_buffer_size,
        &leaf_hashes,
        batch_mint,
    )
}

fn get_leaf_hash(
    asset: &BatchedMintInstruction,
    tree_id: &Pubkey,
) -> Result<[u8; 32], BatchMintValidationError> {
    let asset_id = get_asset_id(tree_id, asset.leaf_update.nonce());
    if asset_id != asset.leaf_update.id() {
        return Err(BatchMintValidationError::PDACheckFail(
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
        return Err(BatchMintValidationError::InvalidDataHash(
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
        return Err(BatchMintValidationError::InvalidCreatorsHash(
            creator_hash.to_string(),
            Hash::new(asset.leaf_update.creator_hash().as_slice()).to_string(),
        ));
    }

    Ok(asset.leaf_update.hash())
}

pub async fn store_batch_mint_update<T>(
    slot: u64,
    signature: String,
    batch_mint: &BatchMint,
    txn: &T,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    for batched_mint in batch_mint.batch_mints.iter() {
        bubblegum::mint_v1::mint_v1(
            &batched_mint.into(),
            &InstructionBundle {
                txn_id: &signature,
                program: Default::default(),
                instruction: None,
                inner_ix: None,
                keys: &[],
                slot,
            },
            txn,
            "CreateTreeWithRoot",
            false,
        )
        .await?;
    }

    Ok(())
}
