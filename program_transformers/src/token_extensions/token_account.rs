use crate::{
    asset_upserts::{upsert_assets_token_account_columns, AssetTokenAccountColumns},
    error::{ProgramTransformerError, ProgramTransformerResult},
    AccountInfo,
};
use blockbuster::programs::token_extensions::TokenAccount;
use digital_asset_types::dao::{asset, sea_orm_active_enums::OwnerType, token_accounts};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, DatabaseConnection, DbBackend, EntityTrait,
};
use solana_sdk::program_option::COption;
use spl_token_2022::state::AccountState;

pub async fn handle_token2022_token_account<'a, 'b, 'c>(
    ta: &TokenAccount,
    account_info: &'a AccountInfo,
    db: &'c DatabaseConnection,
) -> ProgramTransformerResult<()> {
    let slot = account_info.slot as i64;
    let account_key = account_info.pubkey.to_bytes().to_vec();
    let account_owner = account_info.owner.to_bytes().to_vec();
    let mint = ta.account.mint.to_bytes().to_vec();
    let delegate: Option<Vec<u8>> = match ta.account.delegate {
        COption::Some(d) => Some(d.to_bytes().to_vec()),
        COption::None => None,
    };
    let frozen = match ta.account.state {
        AccountState::Frozen => true,
        _ => false,
    };
    let owner = ta.account.owner.to_bytes().to_vec();
    let mut extensions = None;
    if ta.extensions.is_some() {
        extensions = Some(
            serde_json::to_value(ta.extensions.clone())
                .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?,
        );
    }

    let model = token_accounts::ActiveModel {
        pubkey: ActiveValue::Set(account_key.clone()),
        mint: ActiveValue::Set(mint.clone()),
        delegate: ActiveValue::Set(delegate.clone()),
        owner: ActiveValue::Set(owner.clone()),
        frozen: ActiveValue::Set(frozen),
        delegated_amount: ActiveValue::Set(ta.account.delegated_amount as i64),
        token_program: ActiveValue::Set(account_owner.clone()),
        slot_updated: ActiveValue::Set(slot),
        amount: ActiveValue::Set(ta.account.amount as i64),
        close_authority: ActiveValue::Set(None),
        extensions: ActiveValue::Set(extensions),
    };

    let mut query = token_accounts::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([token_accounts::Column::Pubkey])
                .update_columns([
                    token_accounts::Column::Mint,
                    token_accounts::Column::DelegatedAmount,
                    token_accounts::Column::Delegate,
                    token_accounts::Column::Amount,
                    token_accounts::Column::Frozen,
                    token_accounts::Column::TokenProgram,
                    token_accounts::Column::Owner,
                    token_accounts::Column::CloseAuthority,
                    token_accounts::Column::SlotUpdated,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    query.sql = format!(
        "{} WHERE excluded.slot_updated > token_accounts.slot_updated",
        query.sql
    );
    db.execute(query).await?;
    let txn = db.begin().await?;
    let asset_update = asset::Entity::find_by_id(mint.clone())
        .filter(asset::Column::OwnerType.eq(OwnerType::Single))
        .one(&txn)
        .await?;

    if let Some(_asset) = asset_update {
        // will only update owner if token account balance is non-zero
        // since the asset is marked as single then the token account balance can only be 1. Greater implies a fungible token in which case no si
        // TODO: this does not guarantee in case when wallet receives an amount of 1 for a token but its supply is more. is unlikely since mints often have a decimal
        if ta.account.amount == 1 {
            upsert_assets_token_account_columns(
                AssetTokenAccountColumns {
                    mint: mint.clone(),
                    owner: Some(owner.clone()),
                    frozen,
                    delegate,
                    slot_updated_token_account: Some(account_info.slot as i64),
                },
                &txn,
            )
            .await?;
        }
    }

    txn.commit().await?;
    Ok(())
}
