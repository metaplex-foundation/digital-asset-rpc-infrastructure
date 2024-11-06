use crate::{
    asset_upserts::{upsert_assets_mint_account_columns, AssetMintAccountColumns},
    error::{ProgramTransformerError, ProgramTransformerResult},
    AccountInfo,
};
use blockbuster::programs::token_extensions::{extension::ShadowMetadata, MintAccount};

use digital_asset_types::dao::{
    asset, asset_data,
    sea_orm_active_enums::{ChainMutability, OwnerType},
    tokens,
};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ActiveValue, ConnectionTrait, DatabaseConnection,
    DatabaseTransaction, DbBackend, EntityTrait,
};
use solana_sdk::program_option::COption;

pub async fn handle_token2022_mint_account<'a, 'b, 'c>(
    m: &MintAccount,
    account_info: &'a AccountInfo,
    db: &'c DatabaseConnection,
) -> ProgramTransformerResult<()> {
    let account_key = account_info.pubkey.to_bytes().to_vec();
    let account_owner = account_info.owner.to_bytes().to_vec();
    let slot = account_info.slot as i64;
    let txn = db.begin().await?;

    insert_into_tokens_table(
        m,
        account_key.clone(),
        account_owner.clone(),
        account_info.slot as i64,
        &txn,
    )
    .await?;

    upsert_asset(m, account_key.clone(), slot, db).await?;

    if let Some(metadata) = &m.extensions.metadata {
        upsert_asset_data(metadata, account_key.clone(), slot, &txn).await?;
    }

    txn.commit().await?;

    Ok(())
}

async fn insert_into_tokens_table(
    m: &MintAccount,
    account_key: Vec<u8>,
    program: Vec<u8>,
    slot: i64,
    txn: &DatabaseTransaction,
) -> ProgramTransformerResult<()> {
    let extensions = serde_json::to_value(m.extensions.clone())
        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?;
    let freeze_auth: Option<Vec<u8>> = match m.account.freeze_authority {
        COption::Some(d) => Some(d.to_bytes().to_vec()),
        COption::None => None,
    };
    let mint_auth: Option<Vec<u8>> = match m.account.mint_authority {
        COption::Some(d) => Some(d.to_bytes().to_vec()),
        COption::None => None,
    };
    let model = tokens::ActiveModel {
        mint: ActiveValue::Set(account_key.clone()),
        token_program: ActiveValue::Set(program),
        slot_updated: ActiveValue::Set(slot),
        supply: ActiveValue::Set(m.account.supply.into()),
        decimals: ActiveValue::Set(m.account.decimals as i32),
        close_authority: ActiveValue::Set(None),
        extension_data: ActiveValue::Set(None),
        mint_authority: ActiveValue::Set(mint_auth),
        freeze_authority: ActiveValue::Set(freeze_auth),
        extensions: ActiveValue::Set(Some(extensions.clone())),
    };

    let mut query = tokens::Entity::insert(model)
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
    query.sql = format!(
        "{} WHERE excluded.slot_updated >= tokens.slot_updated",
        query.sql
    );

    txn.execute(query).await?;

    Ok(())
}

async fn upsert_asset_data(
    metadata: &ShadowMetadata,
    account_key: Vec<u8>,
    slot: i64,
    txn: &DatabaseTransaction,
) -> ProgramTransformerResult<()> {
    let metadata_json = serde_json::to_value(metadata.clone())
        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?;
    let asset_data_model = asset_data::ActiveModel {
        metadata_url: ActiveValue::Set(metadata.uri.clone()),
        metadata: ActiveValue::Set(JsonValue::String("processing".to_string())),
        id: ActiveValue::Set(account_key.clone()),
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
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    asset_data_query.sql = format!(
        "{} WHERE excluded.slot_updated >= asset_data.slot_updated",
        asset_data_query.sql
    );
    txn.execute(asset_data_query).await?;
    Ok(())
}

async fn upsert_asset(
    m: &MintAccount,
    account_key: Vec<u8>,
    slot: i64,
    db: &DatabaseConnection,
) -> ProgramTransformerResult<()> {
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
                supply_mint: Some(account_key),
                supply: m.account.supply.into(),
                slot_updated_mint_account: slot as u64,
            },
            db,
        )
        .await?;
    }
    Ok(())
}
