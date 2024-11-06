mod mint;
mod token_account;
use crate::{
    error::{ProgramTransformerError, ProgramTransformerResult},
    AccountInfo,
};
use blockbuster::programs::token_extensions::TokenExtensionsProgramAccount;
use das_core::DownloadMetadataNotifier;
use sea_orm::DatabaseConnection;

use self::{mint::handle_token2022_mint_account, token_account::handle_token2022_token_account};

pub async fn handle_token_extensions_program_account<'a, 'b, 'c>(
    account_update: &'a AccountInfo,
    parsing_result: &'b TokenExtensionsProgramAccount,
    db: &'c DatabaseConnection,
    _download_metadata_notifier: &DownloadMetadataNotifier,
) -> ProgramTransformerResult<()> {
    match &parsing_result {
        TokenExtensionsProgramAccount::TokenAccount(ta) => {
            handle_token2022_token_account(ta, account_update, db).await
        }
        TokenExtensionsProgramAccount::MintAccount(m) => {
            handle_token2022_mint_account(m, account_update, db).await
        }
        _ => Err(ProgramTransformerError::NotImplemented),
    }?;
    Ok(())
}
