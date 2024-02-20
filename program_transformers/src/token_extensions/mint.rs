use crate::{
    error::IngesterError,
    program_transformers::{
        asset_upserts::{upsert_assets_token_account_columns, AssetTokenAccountColumns},
        utils::find_model_with_retry,
    },
    tasks::{DownloadMetadata, IntoTaskData, TaskData},
};
use blockbuster::programs::token_extensions::{extension::ShadowMetadata, MintAccount};
use chrono::Utc;
use digital_asset_types::dao::{
    asset, asset_authority, asset_data,
    sea_orm_active_enums::{
        ChainMutability, OwnerType, SpecificationAssetClass, SpecificationVersions,
    },
    token_accounts, tokens,
};
use log::warn;
use plerkle_serialization::AccountInfo;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ActiveValue::Set, ConnectionTrait,
    DatabaseConnection, DatabaseTransaction, DbBackend, DbErr, EntityTrait,
};
use solana_sdk::{program_option::COption, pubkey::Pubkey};

const RETRY_INTERVALS: &[u64] = &[0, 5, 10];

pub async fn handle_token2022_mint_account<'a, 'b, 'c>(
    m: &MintAccount,
    account_update: &'a AccountInfo<'a>,
    db: &'c DatabaseConnection,
) -> Result<Option<TaskData>, IngesterError> {
    let key = *account_update.pubkey().unwrap();
    let key_bytes = key.0.to_vec();
    let spl_token_program = account_update.owner().unwrap().0.to_vec();

    let mut task: Option<TaskData> = None;

    let txn = db.begin().await?;

    insert_into_tokens_table(
        m,
        key_bytes.clone(),
        spl_token_program,
        account_update.slot() as i64,
        &txn,
    )
    .await?;

    if let Some(metadata) = &m.extensions.metadata {
        upsert_asset_data(
            metadata,
            key_bytes.clone(),
            account_update.slot() as i64,
            &txn,
        )
        .await?;

        task = Some(create_task(metadata, key_bytes.clone())?);
    }

    if should_upsert_asset(m) {
        upsert_asset(m, key_bytes.clone(), account_update.slot() as i64, db, &txn).await?;
    }

    txn.commit().await?;

    Ok(task)
}

fn should_upsert_asset(m: &MintAccount) -> bool {
    is_token_nft(m) || m.extensions.metadata.is_some()
}

fn is_token_nft(m: &MintAccount) -> bool {
    m.account.supply == 1 && m.account.decimals == 0
}

async fn insert_into_tokens_table(
    m: &MintAccount,
    key_bytes: Vec<u8>,
    spl_token_program: Vec<u8>,
    slot: i64,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let extensions = serde_json::to_value(m.extensions.clone())
        .map_err(|e| IngesterError::SerializatonError(e.to_string()))?;
    let freeze_auth: Option<Vec<u8>> = match m.account.freeze_authority {
        COption::Some(d) => Some(d.to_bytes().to_vec()),
        COption::None => None,
    };
    let mint_auth: Option<Vec<u8>> = match m.account.mint_authority {
        COption::Some(d) => Some(d.to_bytes().to_vec()),
        COption::None => None,
    };
    let tokens_model = tokens::ActiveModel {
        mint: Set(key_bytes.clone()),
        token_program: Set(spl_token_program),
        slot_updated: Set(slot),
        supply: Set(m.account.supply as i64),
        decimals: Set(m.account.decimals as i32),
        close_authority: Set(None),
        extension_data: Set(None),
        mint_authority: Set(mint_auth),
        freeze_authority: Set(freeze_auth),
        extensions: Set(Some(extensions.clone())),
    };

    let mut tokens_query = tokens::Entity::insert(tokens_model)
        .on_conflict(
            OnConflict::columns([tokens::Column::Mint])
                .update_columns([
                    tokens::Column::Supply,
                    tokens::Column::TokenProgram,
                    tokens::Column::MintAuthority,
                    tokens::Column::CloseAuthority,
                    tokens::Column::Extensions,
                    tokens::Column::SlotUpdated,
                    tokens::Column::Decimals,
                    tokens::Column::FreezeAuthority,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    tokens_query.sql = format!(
        "{} WHERE excluded.slot_updated >= tokens.slot_updated",
        tokens_query.sql
    );

    txn.execute(tokens_query)
        .await
        .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;

    Ok(())
}

async fn upsert_asset_data(
    metadata: &ShadowMetadata,
    key_bytes: Vec<u8>,
    slot: i64,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let metadata_json = serde_json::to_value(metadata.clone())
        .map_err(|e| IngesterError::SerializatonError(e.to_string()))?;
    let asset_data_model = asset_data::ActiveModel {
        metadata_url: Set(metadata.uri.clone()),
        id: Set(key_bytes.clone()),
        chain_data_mutability: Set(ChainMutability::Mutable),
        chain_data: Set(metadata_json),
        slot_updated: Set(slot),
        base_info_seq: Set(Some(0)),
        raw_name: Set(Some(metadata.name.clone().into_bytes().to_vec())),
        raw_symbol: Set(Some(metadata.symbol.clone().into_bytes().to_vec())),
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
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    asset_data_query.sql = format!(
        "{} WHERE excluded.slot_updated >= asset_data.slot_updated",
        asset_data_query.sql
    );
    txn.execute(asset_data_query)
        .await
        .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;
    Ok(())
}

async fn upsert_asset(
    m: &MintAccount,
    key_bytes: Vec<u8>,
    slot: i64,
    db: &DatabaseConnection,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let is_nft = is_token_nft(m);
    let owner_type = match is_nft {
        true => OwnerType::Single,
        false => OwnerType::Token,
    };
    if is_nft {
        let token_account: Option<token_accounts::Model> = find_model_with_retry(
            db,
            "token_accounts",
            &token_accounts::Entity::find()
                .filter(token_accounts::Column::Mint.eq(key_bytes.clone()))
                .filter(token_accounts::Column::Amount.gt(0))
                .order_by(token_accounts::Column::SlotUpdated, Order::Desc),
            RETRY_INTERVALS,
        )
        .await
        .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;

        match token_account {
            Some(ta) => {
                upsert_assets_token_account_columns(
                    AssetTokenAccountColumns {
                        mint: key_bytes.clone(),
                        owner: ta.owner,
                        frozen: ta.frozen,
                        delegate: ta.delegate,
                        slot_updated_token_account: Some(ta.slot_updated),
                    },
                    txn,
                )
                .await?
            }
            None => {
                if m.account.supply == 1 {
                    warn!(
                        target: "Account not found",
                        "Token acc not found in 'token_accounts' table for mint {}",
                        bs58::encode(&key_bytes).into_string()
                    );
                }
            }
        }

        let extensions = serde_json::to_value(m.extensions.clone())
            .map_err(|e| IngesterError::SerializatonError(e.to_string()))?;

        let class = match is_nft {
            true => SpecificationAssetClass::Nft,
            false => SpecificationAssetClass::FungibleToken,
        };

        let mut asset_model = asset::ActiveModel {
            id: Set(key_bytes.clone()),
            owner_type: Set(owner_type),
            supply: Set(m.account.supply as i64),
            supply_mint: Set(Some(key_bytes.clone())),
            specification_version: Set(Some(SpecificationVersions::V1)),
            specification_asset_class: Set(Some(class)),
            nonce: Set(Some(0)),
            seq: Set(Some(0)),
            compressed: Set(false),
            compressible: Set(false),
            asset_data: Set(Some(key_bytes.clone())),
            slot_updated: Set(Some(slot)),
            burnt: Set(false),
            mint_extensions: Set(Some(extensions)),
            ..Default::default()
        };

        let auth_address: Option<Vec<u8>> = m.extensions.metadata.clone().and_then(|m| {
            let auth_pubkey: Option<Pubkey> = m.update_authority.into();
            auth_pubkey.map(|value| value.to_bytes().to_vec())
        });

        if let Some(authority) = auth_address {
            let model = asset_authority::ActiveModel {
                asset_id: Set(key_bytes.clone()),
                authority: Set(authority),
                seq: Set(0),
                slot_updated: Set(slot),
                ..Default::default()
            };

            let mut query = asset_authority::Entity::insert(model)
                .on_conflict(
                    OnConflict::columns([asset_authority::Column::AssetId])
                        .update_columns([
                            asset_authority::Column::Authority,
                            asset_authority::Column::Seq,
                            asset_authority::Column::SlotUpdated,
                        ])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);
            query.sql = format!(
                "{} WHERE excluded.slot_updated > asset_authority.slot_updated",
                query.sql
            );
            txn.execute(query)
                .await
                .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;
        }

        let mut asset_query = asset::Entity::insert(asset_model)
            .on_conflict(
                OnConflict::columns([asset::Column::Id])
                    .update_columns(vec![
                        asset::Column::OwnerType,
                        asset::Column::Supply,
                        asset::Column::SupplyMint,
                        asset::Column::SpecificationVersion,
                        asset::Column::SpecificationAssetClass,
                        asset::Column::Nonce,
                        asset::Column::Seq,
                        asset::Column::Compressed,
                        asset::Column::Compressible,
                        asset::Column::AssetData,
                        asset::Column::SlotUpdated,
                        asset::Column::Burnt,
                    ])
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        asset_query.sql = format!(
        "{} WHERE excluded.slot_updated_mint_account >= asset.slot_updated_mint_account OR asset.slot_updated_mint_account IS NULL",
        asset_query.sql
    );
        txn.execute(asset_query)
            .await
            .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;
    }
    Ok(())
}

fn create_task(metadata: &ShadowMetadata, key_bytes: Vec<u8>) -> Result<TaskData, IngesterError> {
    let mut dm = DownloadMetadata {
        asset_data_id: key_bytes.clone(),
        uri: metadata.uri.clone(),
        created_at: Some(Utc::now().naive_utc()),
    };
    dm.sanitize();
    let td = dm.into_task_data()?;
    Ok(td)
}
