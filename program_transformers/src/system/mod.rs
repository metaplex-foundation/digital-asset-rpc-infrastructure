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
    let is_mint = tokens::Entity::find_by_id(pubkey.clone()).one(db).await?;
    if let Some(mint) = is_mint {
        println!("Deleting mint: {:?}", account.pubkey);
        tokens::Entity::delete_by_id(mint.mint)
            .exec(db)
            .await
            .map_err(|_| {
                ProgramTransformerError::DatabaseError("Failed to delete mint".to_string())
            })?;

        return Ok(());
    }

    let is_token_acc = token_accounts::Entity::find_by_id(pubkey).one(db).await?;
    if let Some(token_acc) = is_token_acc {
        println!("Deleting token account: {:?}", account.pubkey);
        token_accounts::Entity::delete_by_id(token_acc.pubkey)
            .exec(db)
            .await
            .map_err(|_| {
                ProgramTransformerError::DatabaseError("Failed to delete token account".to_string())
            })?;

        return Ok(());
    }

    Ok(())
}
