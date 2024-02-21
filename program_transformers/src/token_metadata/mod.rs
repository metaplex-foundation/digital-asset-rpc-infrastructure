use {
    crate::{
        error::{ProgramTransformerError, ProgramTransformerResult},
        token_metadata::{
            master_edition::{save_v1_master_edition, save_v2_master_edition},
            v1_asset::{burn_v1_asset, save_v1_asset},
        },
        AccountInfo, DownloadMetadataNotifier,
    },
    blockbuster::programs::token_metadata::{TokenMetadataAccountData, TokenMetadataAccountState},
    sea_orm::{DatabaseConnection, TransactionTrait},
};

mod master_edition;
mod v1_asset;

pub async fn handle_token_metadata_account<'a, 'b, 'c>(
    account_info: &'a AccountInfo<'a>,
    parsing_result: &'b TokenMetadataAccountState,
    db: &'c DatabaseConnection,
    download_metadata_notifier: &DownloadMetadataNotifier,
) -> ProgramTransformerResult<()> {
    match &parsing_result.data {
        TokenMetadataAccountData::EmptyAccount => {
            burn_v1_asset(db, *account_info.pubkey, account_info.slot).await?;
            Ok(())
        }
        TokenMetadataAccountData::MasterEditionV1(m) => {
            let txn = db.begin().await?;
            save_v1_master_edition(*account_info.pubkey, account_info.slot, m, &txn).await?;
            txn.commit().await?;
            Ok(())
        }
        TokenMetadataAccountData::MetadataV1(m) => {
            if let Some(info) = save_v1_asset(db, m, account_info.slot).await? {
                download_metadata_notifier(info)
                    .await
                    .map_err(ProgramTransformerError::DownloadMetadataNotify)?;
            }
            Ok(())
        }
        TokenMetadataAccountData::MasterEditionV2(m) => {
            let txn = db.begin().await?;
            save_v2_master_edition(*account_info.pubkey, account_info.slot, m, &txn).await?;
            txn.commit().await?;
            Ok(())
        }
        // TokenMetadataAccountData::EditionMarker(_) => {}
        // TokenMetadataAccountData::UseAuthorityRecord(_) => {}
        // TokenMetadataAccountData::CollectionAuthorityRecord(_) => {}
        _ => Err(ProgramTransformerError::NotImplemented),
    }
}
