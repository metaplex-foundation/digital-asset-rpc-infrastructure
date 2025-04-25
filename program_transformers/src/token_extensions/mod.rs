use {
    crate::{
        asset_upserts::{
            upsert_assets_mint_account_columns, upsert_assets_token_account_columns,
            AssetMintAccountColumns, AssetTokenAccountColumns,
        },
        error::{ProgramTransformerError, ProgramTransformerResult},
        filter_non_null_fields, AccountInfo, DownloadMetadataInfo, DownloadMetadataNotifier,
    },
    blockbuster::programs::token_extensions::{
        extension::ShadowMetadata, MintAccount, TokenAccount, TokenExtensionsProgramEntity,
    },
    digital_asset_types::dao::{
        asset, asset_data,
        sea_orm_active_enums::ChainMutability,
        token_accounts,
        tokens::{self, IsNonFungible as IsNonFungibleModel},
    },
    sea_orm::{
        entity::ActiveValue, query::QueryTrait, sea_query::query::OnConflict, ConnectionTrait,
        DatabaseConnection, DatabaseTransaction, DbBackend, DbErr, EntityTrait, Set,
        TransactionTrait,
    },
    serde_json::Value,
    solana_sdk::program_option::COption,
    spl_token_2022::state::AccountState,
    tracing::warn,
};

pub async fn handle_token_extensions_program_account<'a, 'b, 'c>(
    account_info: &'a AccountInfo,
    parsing_result: &'b TokenExtensionsProgramEntity,
    db: &'c DatabaseConnection,
    download_metadata_notifier: &DownloadMetadataNotifier,
) -> ProgramTransformerResult<()> {
    let account_key = account_info.pubkey.to_bytes().to_vec();
    let account_owner = account_info.owner.to_bytes().to_vec();
    let slot = account_info.slot as i64;
    match parsing_result {
        TokenExtensionsProgramEntity::TokenAccount(ta) => {
            let TokenAccount {
                account,
                extensions,
            } = ta;
            let ta = account;

            let extensions: Option<Value> = if extensions.is_some() {
                filter_non_null_fields(
                    serde_json::to_value(extensions.clone())
                        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?,
                )
            } else {
                None
            };

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
                slot_updated: ActiveValue::Set(slot),
                amount: ActiveValue::Set(ta.amount as i64),
                close_authority: ActiveValue::Set(None),
                extensions: ActiveValue::Set(extensions.clone()),
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
                            token_accounts::Column::Extensions,
                        ])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);
            query.sql = format!(
                "{} WHERE excluded.slot_updated > token_accounts.slot_updated",
                query.sql
            );
            db.execute(query).await?;

            let token = tokens::Entity::find_by_id(mint.clone()).one(db).await?;

            let is_non_fungible = token.map(|t| t.is_non_fungible()).unwrap_or(false);

            if is_non_fungible {
                let txn = db.begin().await?;

                upsert_assets_token_account_columns(
                    AssetTokenAccountColumns {
                        mint: mint.clone(),
                        owner: Some(owner.clone()),
                        frozen,
                        delegate,
                        slot_updated_token_account: Some(slot),
                    },
                    &txn,
                )
                .await?;

                txn.commit().await?;
            }

            Ok(())
        }
        TokenExtensionsProgramEntity::MintAccount(m) => {
            let MintAccount {
                account,
                extensions,
            } = m;

            let mint_extensions: Option<Value> = if extensions.is_some() {
                filter_non_null_fields(
                    serde_json::to_value(extensions.clone())
                        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?,
                )
            } else {
                None
            };

            let m = account;
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
                slot_updated: ActiveValue::Set(slot),
                supply: ActiveValue::Set(m.supply.into()),
                decimals: ActiveValue::Set(m.decimals as i32),
                close_authority: ActiveValue::Set(None),
                extension_data: ActiveValue::Set(None),
                mint_authority: ActiveValue::Set(mint_auth),
                freeze_authority: ActiveValue::Set(freeze_auth),
                extensions: ActiveValue::Set(mint_extensions.clone()),
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
                            tokens::Column::Extensions,
                        ])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);
            query.sql = format!(
                "{} WHERE excluded.slot_updated >= tokens.slot_updated",
                query.sql
            );
            db.execute(query).await?;
            let txn = db.begin().await?;

            upsert_assets_mint_account_columns(
                AssetMintAccountColumns {
                    mint: account_key.clone(),
                    supply: m.supply.into(),
                    slot_updated_mint_account: slot,
                    extensions: mint_extensions.clone(),
                },
                &txn,
            )
            .await?;

            txn.commit().await?;

            if let Some(metadata) = &extensions.metadata {
                if let Some(info) =
                    upsert_asset_data(metadata, account_key.clone(), slot, db).await?
                {
                    download_metadata_notifier(info)
                        .await
                        .map_err(ProgramTransformerError::DownloadMetadataNotify)?;
                }
            }

            Ok(())
        }
        _ => Err(ProgramTransformerError::NotImplemented),
    }
}

async fn upsert_asset_data(
    metadata: &ShadowMetadata,
    key_bytes: Vec<u8>,
    slot: i64,
    db: &DatabaseConnection,
) -> ProgramTransformerResult<Option<DownloadMetadataInfo>> {
    let metadata_json = serde_json::to_value(metadata.clone())
        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?;
    let asset_data_model = asset_data::ActiveModel {
        metadata_url: ActiveValue::Set(metadata.uri.clone()),
        metadata: ActiveValue::Set(Value::String("processing".to_string())),
        id: ActiveValue::Set(key_bytes.clone()),
        chain_data_mutability: ActiveValue::Set(ChainMutability::Mutable),
        chain_data: ActiveValue::Set(metadata_json),
        slot_updated: ActiveValue::Set(slot),
        base_info_seq: ActiveValue::Set(Some(0)),
        raw_name: ActiveValue::Set(Some(metadata.name.clone().into_bytes().to_vec())),
        raw_symbol: ActiveValue::Set(Some(metadata.symbol.clone().into_bytes().to_vec())),
        ..Default::default()
    };
    let mut asset_data_query = asset_data::Entity::insert(asset_data_model)
        .on_conflict(
            OnConflict::columns([asset_data::Column::Id])
                .update_columns([
                    asset_data::Column::ChainDataMutability,
                    asset_data::Column::ChainData,
                    asset_data::Column::MetadataUrl,
                    asset_data::Column::SlotUpdated,
                    asset_data::Column::BaseInfoSeq,
                    asset_data::Column::RawName,
                    asset_data::Column::RawSymbol,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    asset_data_query.sql = format!(
        "{} WHERE excluded.slot_updated >= asset_data.slot_updated",
        asset_data_query.sql
    );
    db.execute(asset_data_query).await?;

    let txn = db.begin().await?;
    upsert_assets_metadata_cols(
        AssetMetadataAccountCols {
            mint: key_bytes.clone(),
            slot_updated_metadata_account: slot,
        },
        &txn,
    )
    .await?;

    txn.commit().await?;

    if metadata.uri.is_empty() {
        warn!(
            "URI is empty for mint {}. Skipping background task.",
            bs58::encode(key_bytes).into_string()
        );
        return Ok(None);
    }

    Ok(Some(DownloadMetadataInfo::new(
        key_bytes,
        metadata.uri.clone(),
    )))
}

struct AssetMetadataAccountCols {
    mint: Vec<u8>,
    slot_updated_metadata_account: i64,
}

async fn upsert_assets_metadata_cols(
    metadata: AssetMetadataAccountCols,
    db: &DatabaseTransaction,
) -> Result<(), DbErr> {
    let asset = asset::ActiveModel {
        id: ActiveValue::Set(metadata.mint.clone()),
        slot_updated_metadata_account: Set(Some(metadata.slot_updated_metadata_account)),
        ..Default::default()
    };

    let mut asset_query = asset::Entity::insert(asset)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([asset::Column::SlotUpdatedMetadataAccount])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    asset_query.sql = format!(
        "{} WHERE excluded.slot_updated_metadata_account >= asset.slot_updated_metadata_account OR asset.slot_updated_metadata_account IS NULL",
        asset_query.sql
    );

    db.execute(asset_query).await?;

    Ok(())
}
