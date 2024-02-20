use {
    crate::{
        asset_upserts::{
            upsert_assets_mint_account_columns, upsert_assets_token_account_columns,
            AssetMintAccountColumns, AssetTokenAccountColumns,
        },
        error::{ProgramTransformerError, ProgramTransformerResult},
        DownloadMetadataNotifier,
    },
    blockbuster::programs::token_account::TokenProgramAccount,
    digital_asset_types::dao::{asset, sea_orm_active_enums::OwnerType, token_accounts, tokens},
    plerkle_serialization::AccountInfo,
    sea_orm::{
        entity::{ActiveValue, ColumnTrait},
        query::{QueryFilter, QueryTrait},
        sea_query::query::OnConflict,
        ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, Set, TransactionTrait,
    },
    solana_sdk::program_option::COption,
    spl_token::state::AccountState,
};

pub async fn upsert_owner_for_token_account<T>(
    txn_or_conn: &T,
    id: Vec<u8>,
    token_account: Vec<u8>,
    owner: Vec<u8>,
    delegate: Option<Vec<u8>>,
    slot: i64,
    frozen: bool,
    amount: u64,
    delegate_amount: i64,
    token_program: Vec<u8>,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let model = token_accounts::ActiveModel {
        pubkey: Set(token_account),
        mint: Set(id),
        delegate: Set(delegate.clone()),
        owner: Set(owner.clone()),
        frozen: Set(frozen),
        delegated_amount: Set(delegate_amount),
        token_program: Set(token_program),
        slot_updated: Set(slot),
        amount: Set(amount as i64),
        close_authority: Set(None),
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
        "{} WHERE excluded.slot_updated >= token_accounts.slot_updated OR token_accounts.slot_updated IS NULL",
        query.sql
    );
    txn_or_conn
        .execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;
    Ok(())
}

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

            upsert_owner_for_token_account(
                db,
                mint.clone(),
                key_bytes,
                owner.clone(),
                delegate.clone(),
                account_update.slot() as i64,
                frozen,
                ta.amount,
                ta.delegated_amount as i64,
                spl_token_program,
            )
            .await?;

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
                            slot_updated_token_account: Some(account_update.slot() as i64),
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
                mint: ActiveValue::Set(key_bytes.clone()),
                token_program: ActiveValue::Set(spl_token_program),
                slot_updated: ActiveValue::Set(account_update.slot() as i64),
                supply: ActiveValue::Set(m.supply as i64),
                decimals: ActiveValue::Set(m.decimals as i32),
                close_authority: ActiveValue::Set(None),
                extension_data: ActiveValue::Set(None),
                mint_authority: ActiveValue::Set(mint_auth),
                freeze_authority: ActiveValue::Set(freeze_auth),
                extensions: ActiveValue::Set(None),
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

            let asset_update: Option<asset::Model> = asset::Entity::find_by_id(key_bytes.clone())
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
                        mint: key_bytes.clone(),
                        suppply_mint: Some(key_bytes),
                        supply: m.supply,
                        slot_updated_mint_account: account_update.slot(),
                    },
                    db,
                )
                .await?;
            }

            Ok(())
        }
        _ => Err(ProgramTransformerError::NotImplemented),
    }?;
    Ok(())
}
