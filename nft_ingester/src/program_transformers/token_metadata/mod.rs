mod master_edition;
mod v1_asset;

use crate::{
    program_transformers::token_metadata::{
        master_edition::{save_v1_master_edition, save_v2_master_edition},
        v1_asset::{burn_v1_asset, save_v1_asset},
    },
    error::IngesterError, tasks::TaskData,
};
use blockbuster::programs::token_metadata::{TokenMetadataAccountData, TokenMetadataAccountState};
use plerkle_serialization::AccountInfo;
use sea_orm::{DatabaseConnection, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

pub async fn handle_token_metadata_account<'a, 'b, 'c>(
    account_update: &'a AccountInfo<'a>,
    parsing_result: &'b TokenMetadataAccountState,
    db: &'c DatabaseConnection,
    task_manager: &UnboundedSender<TaskData>,
) -> Result<(), IngesterError> {
    let key = *account_update.pubkey().unwrap();
    match &parsing_result.data {
        TokenMetadataAccountData::EmptyAccount => {
            burn_v1_asset(db, key, account_update.slot()).await?;
            Ok(())
        }
        TokenMetadataAccountData::MasterEditionV1(m) => {
            let txn = db.begin().await?;
            save_v1_master_edition(key, account_update.slot(), m, &txn).await?;
            txn.commit().await?;
            Ok(())
        }
        TokenMetadataAccountData::MetadataV1(m) => {
            let task = save_v1_asset(db, m.mint.as_ref().into(), account_update.slot(), m).await?;
            task_manager.send(task)?;
            Ok(())
        }
        TokenMetadataAccountData::MasterEditionV2(m) => {
            let txn = db.begin().await?;
            save_v2_master_edition(key, account_update.slot(), m, &txn).await?;
            txn.commit().await?;
            Ok(())
        }
        // TokenMetadataAccountData::EditionMarker(_) => {}
        // TokenMetadataAccountData::UseAuthorityRecord(_) => {}
        // TokenMetadataAccountData::CollectionAuthorityRecord(_) => {}
        _ => Err(IngesterError::NotImplemented),
    }?;
    Ok(())
}
