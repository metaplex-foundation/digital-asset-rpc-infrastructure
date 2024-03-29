mod mint;
mod token_account;
use crate::{
    error::{ProgramTransformerError, ProgramTransformerResult},
    AccountInfo, DownloadMetadataNotifier,
};
use blockbuster::programs::token_extensions::TokenExtensionsProgramAccount;
use sea_orm::DatabaseConnection;

use self::{mint::handle_token2022_mint_account, token_account::handle_token2022_token_account};

pub async fn handle_token_extensions_program_account<'a, 'b>(
    account_update: &AccountInfo,
    parsing_result: &'a TokenExtensionsProgramAccount,
    db: &'b DatabaseConnection,
    download_metadata_notifier: &DownloadMetadataNotifier,
) -> ProgramTransformerResult<()> {
    match &parsing_result {
        TokenExtensionsProgramAccount::TokenAccount(ta) => {
            handle_token2022_token_account(ta, account_update, db).await?;
            Ok(())
        }
        TokenExtensionsProgramAccount::MintAccount(m) => {
            let task = handle_token2022_mint_account(m, account_update, db).await?;
            if let Some(info) = task {
                download_metadata_notifier(info)
                    .await
                    .map_err(ProgramTransformerError::DownloadMetadataNotify)?;
            }
            Ok(())
        }
        _ => Err(ProgramTransformerError::NotImplemented),
    }?;
    Ok(())
}
