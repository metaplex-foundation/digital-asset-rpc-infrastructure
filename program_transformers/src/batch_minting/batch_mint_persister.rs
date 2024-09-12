use async_channel::Receiver;
use std::sync::Arc;

use crate::bubblegum;
use crate::error::{BatchMintValidationError, ProgramTransformerError, ProgramTransformerResult};
use async_trait::async_trait;
use blockbuster::instruction::InstructionBundle;
use bubblegum_batch_sdk::{batch_mint_validations::validate_batch_mint, model::BatchMint};
use cadence_macros::{statsd_count, statsd_histogram};
use digital_asset_types::dao::sea_orm_active_enums::{
    BatchMintFailStatus, BatchMintPersistingState,
};
use digital_asset_types::dao::{batch_mint, batch_mint_to_verify};
use mockall::automock;
use sea_orm::sea_query::{LockType, OnConflict};
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbBackend, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, QuerySelect, QueryTrait, TransactionTrait,
};
use solana_sdk::pubkey::Pubkey;
use tokio::time::Instant;
use tracing::{error, info};

pub const MAX_BATCH_MINT_DOWNLOAD_ATTEMPTS: u8 = 5;

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
                statsd_count!("batch_mint.fail_get_batch_mint", 1);
                continue;
            };
            let Some(batch_mint_to_verify) = batch_mint_to_verify else {
                // no batch mints to persist
                continue;
            };
            let batch_mint = batch_mint
                .map(|r| bincode::deserialize::<BatchMint>(r.batch_mint_binary_bincode.as_slice()))
                .transpose()
                .unwrap_or_default();
            self.persist_batch_mint(batch_mint_to_verify, batch_mint.map(Box::new))
                .await;
        }
    }

    pub async fn persist_batch_mint(
        &self,
        mut batch_mint_to_verify: batch_mint_to_verify::Model,
        mut batch_mint: Option<Box<BatchMint>>,
    ) {
        let start_time = Instant::now();
        info!("Persisting {} batch mint", &batch_mint_to_verify.url);
        loop {
            match &batch_mint_to_verify.batch_mint_persisting_state {
                &BatchMintPersistingState::ReceivedTransaction => {
                    // We get ReceivedTransaction state on the start of processing
                    batch_mint_to_verify.batch_mint_persisting_state =
                        BatchMintPersistingState::StartProcessing;
                }
                &BatchMintPersistingState::StartProcessing => {
                    if let Err(err) = self
                        .download_batch_mint(&mut batch_mint_to_verify, &mut batch_mint)
                        .await
                    {
                        error!("Error during batch mint downloading: {}", err)
                    };
                }
                &BatchMintPersistingState::SuccessfullyDownload => {
                    if let Some(r) = &batch_mint {
                        self.validate_batch_mint(&mut batch_mint_to_verify, r).await;
                    } else {
                        error!(
                            "Trying to validate non downloaded batch mint: {:#?}",
                            &batch_mint_to_verify
                        )
                    }
                }
                &BatchMintPersistingState::SuccessfullyValidate => {
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
                &BatchMintPersistingState::FailedToPersist
                | &BatchMintPersistingState::StoredUpdate => {
                    if let Err(e) = self.drop_batch_mint_from_queue(&batch_mint_to_verify).await {
                        error!("failed to drop batch mint from queue: {}", e);
                    };
                    info!(
                        "Finish processing {} batch mint file with {:?} state",
                        &batch_mint_to_verify.url,
                        &batch_mint_to_verify.batch_mint_persisting_state
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
    ) -> Result<
        (
            Option<batch_mint_to_verify::Model>,
            Option<batch_mint::Model>,
        ),
        ProgramTransformerError,
    > {
        let multi_txn = self.txn.begin().await?;
        let condition = Condition::all()
            .add(
                batch_mint_to_verify::Column::BatchMintPersistingState
                    .ne(BatchMintPersistingState::FailedToPersist),
            )
            .add(
                batch_mint_to_verify::Column::BatchMintPersistingState
                    .ne(BatchMintPersistingState::StoredUpdate),
            )
            .add(
                batch_mint_to_verify::Column::BatchMintPersistingState
                    .ne(BatchMintPersistingState::StartProcessing),
            );

        let batch_mint_verify = batch_mint_to_verify::Entity::find()
            .filter(condition)
            .order_by_asc(batch_mint_to_verify::Column::CreatedAtSlot)
            .lock(LockType::Update)
            .one(&multi_txn)
            .await
            .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
        let mut batch_mint = None;
        if let Some(ref r) = batch_mint_verify {
            batch_mint = batch_mint::Entity::find()
                .filter(batch_mint::Column::FileHash.eq(r.file_hash.clone()))
                .one(self.txn.as_ref())
                .await
                .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
            batch_mint_to_verify::Entity::update(batch_mint_to_verify::ActiveModel {
                file_hash: Set(r.file_hash.clone()),
                url: Set(r.url.clone()),
                created_at_slot: Set(r.created_at_slot),
                signature: Set(r.signature.clone()),
                merkle_tree: Set(r.merkle_tree.clone()),
                staker: Set(r.staker.clone()),
                download_attempts: Set(r.download_attempts),
                batch_mint_persisting_state: Set(BatchMintPersistingState::StartProcessing),
                batch_mint_fail_status: Set(r.batch_mint_fail_status.clone()),
                collection: Set(r.collection.clone()),
            })
            .filter(batch_mint_to_verify::Column::FileHash.eq(r.file_hash.clone()))
            .exec(&multi_txn)
            .await
            .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
        }
        multi_txn.commit().await?;

        Ok((batch_mint_verify, batch_mint))
    }

    async fn download_batch_mint(
        &self,
        batch_mint_to_verify: &mut batch_mint_to_verify::Model,
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
                let query = batch_mint::Entity::insert(batch_mint::ActiveModel {
                    file_hash: Set(batch_mint_to_verify.file_hash.clone()),
                    batch_mint_binary_bincode: Set(bincode::serialize(&r)
                        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?),
                })
                .on_conflict(
                    OnConflict::columns([batch_mint::Column::FileHash])
                        .update_columns([batch_mint::Column::BatchMintBinaryBincode])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);
                if let Err(e) = self.txn.execute(query).await {
                    return Err(e.into());
                }
                *batch_mint = Some(r);
                batch_mint_to_verify.batch_mint_persisting_state =
                    BatchMintPersistingState::SuccessfullyDownload;
                statsd_count!("batch_mint.successfully_download", 1);
            }
            Err(e) => {
                statsd_count!("batch_mint.download_fail", 1);
                if let BatchMintValidationError::InvalidDataHash(expected, actual) = e {
                    batch_mint_to_verify.batch_mint_persisting_state =
                        BatchMintPersistingState::FailedToPersist;
                    self.save_batch_mint_as_failed(
                        BatchMintFailStatus::ChecksumVerifyFailed,
                        batch_mint_to_verify,
                    )
                    .await?;

                    statsd_count!("batch_mint.checksum_verify_fail", 1);
                    return Err(ProgramTransformerError::BatchMintValidation(
                        BatchMintValidationError::FileChecksumMismatch(expected, actual),
                    ));
                }
                if let BatchMintValidationError::Serialization(e) = e {
                    batch_mint_to_verify.batch_mint_persisting_state =
                        BatchMintPersistingState::FailedToPersist;
                    self.save_batch_mint_as_failed(
                        BatchMintFailStatus::FileSerialization,
                        batch_mint_to_verify,
                    )
                    .await?;

                    statsd_count!("batch_mint.file_deserialization_fail", 1);
                    return Err(ProgramTransformerError::SerializatonError(e));
                }
                if batch_mint_to_verify.download_attempts + 1
                    > MAX_BATCH_MINT_DOWNLOAD_ATTEMPTS as i32
                {
                    batch_mint_to_verify.batch_mint_persisting_state =
                        BatchMintPersistingState::FailedToPersist;
                    self.save_batch_mint_as_failed(
                        BatchMintFailStatus::DownloadFailed,
                        batch_mint_to_verify,
                    )
                    .await?;
                } else {
                    batch_mint_to_verify.download_attempts =
                        batch_mint_to_verify.download_attempts + 1;
                    if let Err(e) = (batch_mint_to_verify::ActiveModel {
                        file_hash: Set(batch_mint_to_verify.file_hash.clone()),
                        url: Set(batch_mint_to_verify.url.clone()),
                        created_at_slot: Set(batch_mint_to_verify.created_at_slot),
                        signature: Set(batch_mint_to_verify.signature.clone()),
                        merkle_tree: Set(batch_mint_to_verify.merkle_tree.clone()),
                        staker: Set(batch_mint_to_verify.staker.clone()),
                        download_attempts: Set(batch_mint_to_verify.download_attempts + 1),
                        batch_mint_persisting_state: Set(batch_mint_to_verify
                            .batch_mint_persisting_state
                            .clone()),
                        batch_mint_fail_status: Set(batch_mint_to_verify
                            .batch_mint_fail_status
                            .clone()),
                        collection: Set(batch_mint_to_verify.collection.clone()),
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
        batch_mint_to_verify: &mut batch_mint_to_verify::Model,
        batch_mint: &BatchMint,
    ) {
        let collection_raw_key: Option<[u8; 32]> =
            if let Some(key) = batch_mint_to_verify.collection.clone() {
                match key.try_into() {
                    Ok(key_as_array) => Some(key_as_array),
                    Err(e) => {
                        error!("Could not convert collection key received from DB: {:?}", e);

                        self.mark_persisting_failed(batch_mint_to_verify).await;
                        return;
                    }
                }
            } else {
                None
            };

        if let Err(e) =
            validate_batch_mint(batch_mint, collection_raw_key.map(|key| Pubkey::from(key))).await
        {
            error!("Error while validating batch mint: {}", e.to_string());

            self.mark_persisting_failed(batch_mint_to_verify).await;
            return;
        }
        statsd_count!("batch_mint.validating_success", 1);
        batch_mint_to_verify.batch_mint_persisting_state =
            BatchMintPersistingState::SuccessfullyValidate;
    }

    async fn mark_persisting_failed(&self, batch_mint_to_verify: &mut batch_mint_to_verify::Model) {
        statsd_count!("batch_mint.validating_fail", 1);
        batch_mint_to_verify.batch_mint_persisting_state =
            BatchMintPersistingState::FailedToPersist;
        if let Err(err) = self
            .save_batch_mint_as_failed(
                BatchMintFailStatus::BatchMintVerifyFailed,
                batch_mint_to_verify,
            )
            .await
        {
            error!("Save batch mint as failed: {}", err);
        }
    }

    async fn store_batch_mint_update(
        &self,
        batch_mint_to_verify: &mut batch_mint_to_verify::Model,
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
            batch_mint_to_verify.batch_mint_persisting_state =
                BatchMintPersistingState::FailedToPersist;
            return Ok(());
        }
        statsd_count!("batch_mint.store_update_success", 1);
        batch_mint_to_verify.batch_mint_persisting_state = BatchMintPersistingState::StoredUpdate;
        Ok(())
    }

    async fn save_batch_mint_as_failed(
        &self,
        status: BatchMintFailStatus,
        batch_mint: &batch_mint_to_verify::Model,
    ) -> Result<(), ProgramTransformerError> {
        batch_mint_to_verify::Entity::update(batch_mint_to_verify::ActiveModel {
            file_hash: Set(batch_mint.file_hash.clone()),
            url: Set(batch_mint.url.clone()),
            created_at_slot: Set(batch_mint.created_at_slot),
            signature: Set(batch_mint.signature.clone()),
            merkle_tree: Set(batch_mint.merkle_tree.clone()),
            staker: Set(batch_mint.staker.clone()),
            download_attempts: Set(batch_mint.download_attempts),
            batch_mint_persisting_state: Set(batch_mint.batch_mint_persisting_state.clone()),
            batch_mint_fail_status: Set(Some(status)),
            collection: Set(batch_mint.collection.clone()),
        })
        .filter(batch_mint_to_verify::Column::FileHash.eq(batch_mint.file_hash.clone()))
        .exec(self.txn.as_ref())
        .await
        .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn drop_batch_mint_from_queue(
        &self,
        batch_mint_to_verify: &batch_mint_to_verify::Model,
    ) -> Result<(), ProgramTransformerError> {
        batch_mint_to_verify::Entity::update(batch_mint_to_verify::ActiveModel {
            batch_mint_persisting_state: Set(batch_mint_to_verify
                .batch_mint_persisting_state
                .clone()),
            ..batch_mint_to_verify.clone().into_active_model()
        })
        .filter(batch_mint_to_verify::Column::FileHash.eq(batch_mint_to_verify.file_hash.clone()))
        .exec(self.txn.as_ref())
        .await
        .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;

        Ok(())
    }
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
            // only signature and slot will be used
            &InstructionBundle {
                txn_id: &signature,
                slot,
                ..Default::default()
            },
            txn,
            "FinalizeTreeWithRoot",
            false,
        )
        .await?;
    }

    Ok(())
}
