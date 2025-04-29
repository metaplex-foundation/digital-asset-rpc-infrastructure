use crate::AccountInfo;

use digital_asset_types::dao::{token_accounts, tokens};

use sea_orm::{DatabaseConnection, EntityTrait};

use crate::error::{ProgramTransformerError, ProgramTransformerResult};

// This function handles the system program account and used to close mints and token accounts.
pub async fn handle_system_program_account(
    account: &AccountInfo,
    db: &DatabaseConnection,
) -> ProgramTransformerResult<()> {
    if !account.data.is_empty() {
        return Err(ProgramTransformerError::NotImplemented);
    }

    let pubkey = account.pubkey.to_bytes().to_vec();

    // Try to delete the mint first
    let mint_delete_res = tokens::Entity::delete_by_id(pubkey.clone())
        .exec(db)
        .await?;

    if mint_delete_res.rows_affected == 0 {
        // If no rows were deleted, try to delete the token account
        token_accounts::Entity::delete_by_id(pubkey)
            .exec(db)
            .await?;
    }

    Ok(())
}
