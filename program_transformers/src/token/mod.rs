use {
    crate::{
        asset_upserts::{
            upsert_assets_mint_account_columns, upsert_assets_token_account_columns,
            AssetMintAccountColumns, AssetTokenAccountColumns,
        },
        error::ProgramTransformerResult,
        AccountInfo, DownloadMetadataNotifier,
    },
    blockbuster::programs::token_account::TokenProgramAccount,
    digital_asset_types::dao::{asset, sea_orm_active_enums::OwnerType, token_accounts, tokens},
    sea_orm::{
        entity::{ActiveValue, ColumnTrait},
        query::{QueryFilter, QueryTrait},
        sea_query::query::OnConflict,
        ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, TransactionTrait,
    },
    solana_sdk::program_option::COption,
    spl_token::state::AccountState,
};

pub async fn handle_token_program_account<'a, 'b>(
    account_info: &AccountInfo,
    parsing_result: &'a TokenProgramAccount,
    db: &'b DatabaseConnection,
    _download_metadata_notifier: &DownloadMetadataNotifier,
) -> ProgramTransformerResult<()> {
    let account_key = account_info.pubkey.to_bytes().to_vec();
    let account_owner = account_info.owner.to_bytes().to_vec();
    match &parsing_result {
        TokenProgramAccount::TokenAccount(ta) => {
            let mint = ta.mint.to_bytes().to_vec();
            let delegate: Option<Vec<u8>> = match ta.delegate {
                COption::Some(d) => Some(d.to_bytes().to_vec()),
                COption::None => None,
            };
            let frozen = matches!(ta.state, AccountState::Frozen);
            let owner = ta.owner.to_bytes().to_vec();
            let model = token_accounts::ActiveModel {
                pubkey: ActiveValue::Set(account_key.clone()),
                mint: ActiveValue::Set(mint.clone()),
                delegate: ActiveValue::Set(delegate.clone()),
                owner: ActiveValue::Set(owner.clone()),
                frozen: ActiveValue::Set(frozen),
                delegated_amount: ActiveValue::Set(ta.delegated_amount as i64),
                token_program: ActiveValue::Set(account_owner.clone()),
                slot_updated: ActiveValue::Set(account_info.slot as i64),
                amount: ActiveValue::Set(ta.amount as i64),
                close_authority: ActiveValue::Set(None),
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
            let asset_update: Option<asset::Model> = asset::Entity::find_by_id(mint.clone())
                .filter(asset::Column::OwnerType.eq("single"))
                .one(&txn)
                .await?;
            if let Some(_asset) = asset_update {
                // will only update owner if token account balance is non-zero
                // since the asset is marked as single then the token account balance can only be 1. Greater implies a fungible token in which case no si
                // TODO: this does not guarantee in case when wallet receives an amount of 1 for a token but its supply is more. is unlikely since mints often have a decimal
                if ta.amount == 1 {
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
        TokenProgramAccount::Mint(m) => {
            let freeze_auth: Option<Vec<u8>> = match m.freeze_authority {
                COption::Some(d) => Some(d.to_bytes().to_vec()),
                COption::None => None,
            };
            let mint_auth: Option<Vec<u8>> = match m.mint_authority {
                COption::Some(d) => Some(d.to_bytes().to_vec()),
                COption::None => None,
            };
            let model = tokens::ActiveModel {
                mint: ActiveValue::Set(account_key.clone()),
                token_program: ActiveValue::Set(account_owner),
                slot_updated: ActiveValue::Set(account_info.slot as i64),
                supply: ActiveValue::Set(m.supply as i64),
                decimals: ActiveValue::Set(m.decimals as i32),
                close_authority: ActiveValue::Set(None),
                extension_data: ActiveValue::Set(None),
                mint_authority: ActiveValue::Set(mint_auth),
                freeze_authority: ActiveValue::Set(freeze_auth),
            };

            let mut query = tokens::Entity::insert(model)
                .on_conflict(
                    OnConflict::columns([tokens::Column::Mint])
                        .update_columns([
                            tokens::Column::Supply,
                            tokens::Column::TokenProgram,
                            tokens::Column::MintAuthority,
                            tokens::Column::CloseAuthority,
                            tokens::Column::ExtensionData,
                            tokens::Column::SlotUpdated,
                            tokens::Column::Decimals,
                            tokens::Column::FreezeAuthority,
                        ])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);
            query.sql = format!(
                "{} WHERE excluded.slot_updated >= tokens.slot_updated",
                query.sql
            );
            db.execute(query).await?;

            let asset_update: Option<asset::Model> = asset::Entity::find_by_id(account_key.clone())
                .filter(
                    asset::Column::OwnerType
                        .eq(OwnerType::Single)
                        .or(asset::Column::OwnerType
                            .eq(OwnerType::Unknown)
                            .and(asset::Column::Supply.eq(1))),
                )
                .one(db)
                .await?;
            if let Some(_asset) = asset_update {
                upsert_assets_mint_account_columns(
                    AssetMintAccountColumns {
                        mint: account_key.clone(),
                        suppply_mint: Some(account_key),
                        supply: m.supply,
                        slot_updated_mint_account: account_info.slot,
                    },
                    db,
                )
                .await?;
            }

            Ok(())
        }
    }
}
