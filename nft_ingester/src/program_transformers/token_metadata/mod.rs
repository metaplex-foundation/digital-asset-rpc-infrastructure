mod v1_asset;

use plerkle_serialization::AccountInfo;
use crate::{BgTask, IngesterError};
use blockbuster::programs::token_metadata::{TokenMetadataAccountData, TokenMetadataAccountState};
use sea_orm::{DatabaseConnection, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;
use crate::program_transformers::token_metadata::v1_asset::save_v1_asset;

pub async fn handle_token_metadata_account<'a, 'b, 'c>(
    account_update: &'a AccountInfo<'a>,
    parsing_result: &'b TokenMetadataAccountState,
    db: &'c DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let txn = db.begin().await?;
    let key = account_update.pubkey().unwrap().clone();
    match &parsing_result.data {
        // TokenMetadataAccountData::EditionV1(e) => {}
        // TokenMetadataAccountData::MasterEditionV1(e) => {}
        TokenMetadataAccountData::MetadataV1(m) => {
            println!("Got one!");
            let task = save_v1_asset(key, account_update.slot(), &Default::default(), &txn).await?;
            task_manager.send(Box::new(task))?;
            txn.commit().await?;
            Ok(())
        }
        // TokenMetadataAccountData::MasterEditionV2(e) => {}
        // TokenMetadataAccountData::EditionMarker(_) => {}
        // TokenMetadataAccountData::UseAuthorityRecord(_) => {}
        // TokenMetadataAccountData::CollectionAuthorityRecord(_) => {}
        _ => Err(IngesterError::NotImplemented),
    }
}


