use {
    crate::{
        error::{ProgramTransformerError, ProgramTransformerResult},
        DownloadMetadataNotifier,
    },
    blockbuster::programs::token_account::TokenProgramAccount,
    digital_asset_types::dao::{asset, token_accounts, tokens},
    plerkle_serialization::AccountInfo,
    sea_orm::{
        entity::{ActiveModelTrait, ActiveValue, ColumnTrait},
        query::{QueryFilter, QueryTrait},
        sea_query::query::OnConflict,
        ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, TransactionTrait,
    },
    solana_sdk::program_option::COption,
    spl_token::state::AccountState,
};

pub async fn handle_token_program_account<'a, 'b, 'c>(
    account_update: &'a AccountInfo<'a>,
    parsing_result: &'b TokenProgramAccount,
    db: &'c DatabaseConnection,
    _download_metadata_notifier: &DownloadMetadataNotifier,
) -> ProgramTransformerResult<()> {
    let key = *account_update.pubkey().unwrap();
    let key_bytes = key.0.to_vec();
    let spl_token_program = account_update.owner().unwrap().0.to_vec();
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
                pubkey: ActiveValue::Set(key_bytes),
                mint: ActiveValue::Set(mint.clone()),
                delegate: ActiveValue::Set(delegate.clone()),
                owner: ActiveValue::Set(owner.clone()),
                frozen: ActiveValue::Set(frozen),
                delegated_amount: ActiveValue::Set(ta.delegated_amount as i64),
                token_program: ActiveValue::Set(spl_token_program),
                slot_updated: ActiveValue::Set(account_update.slot() as i64),
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
            let asset_update: Option<asset::Model> = asset::Entity::find_by_id(mint)
                .filter(asset::Column::OwnerType.eq("single"))
                .one(&txn)
                .await?;
            if let Some(asset) = asset_update {
                // will only update owner if token account balance is non-zero
                if ta.amount > 0 {
                    let mut active: asset::ActiveModel = asset.into();
                    active.owner = ActiveValue::Set(Some(owner));
                    active.delegate = ActiveValue::Set(delegate);
                    active.frozen = ActiveValue::Set(frozen);
                    active.save(&txn).await?;
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
                mint: ActiveValue::Set(key_bytes.clone()),
                token_program: ActiveValue::Set(spl_token_program),
                slot_updated: ActiveValue::Set(account_update.slot() as i64),
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
                "{} WHERE excluded.slot_updated > tokens.slot_updated",
                query.sql
            );
            db.execute(query).await?;
            let asset_update: Option<asset::Model> = asset::Entity::find_by_id(key_bytes.clone())
                .filter(asset::Column::OwnerType.eq("single"))
                .one(db)
                .await?;
            if let Some(asset) = asset_update {
                let mut active: asset::ActiveModel = asset.into();
                active.supply = ActiveValue::Set(m.supply as i64);
                active.supply_mint = ActiveValue::Set(Some(key_bytes));
                active.save(db).await?;
            }
            Ok(())
        }
        _ => Err(ProgramTransformerError::NotImplemented),
    }?;
    Ok(())
}
