use {
    digital_asset_types::dao::{
        asset,
        sea_orm_active_enums::{
            OwnerType, RoyaltyTargetType, SpecificationAssetClass, SpecificationVersions,
        },
    },
    sea_orm::{
        sea_query::OnConflict, ConnectionTrait, DbBackend, DbErr, EntityTrait, QueryTrait, Set,
        TransactionTrait,
    },
    serde_json::value::Value,
    sqlx::types::Decimal,
};

pub struct AssetTokenAccountColumns {
    pub mint: Vec<u8>,
    pub owner: Option<Vec<u8>>,
    pub frozen: bool,
    pub delegate: Option<Vec<u8>>,
    pub slot_updated_token_account: Option<i64>,
}

pub async fn upsert_assets_token_account_columns<T: ConnectionTrait + TransactionTrait>(
    columns: AssetTokenAccountColumns,
    txn_or_conn: &T,
) -> Result<(), DbErr> {
    let active_model = asset::ActiveModel {
        id: Set(columns.mint),
        owner: Set(columns.owner),
        frozen: Set(columns.frozen),
        delegate: Set(columns.delegate),
        slot_updated_token_account: Set(columns.slot_updated_token_account),
        ..Default::default()
    };
    let mut query = asset::Entity::insert(active_model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::Owner,
                    asset::Column::Frozen,
                    asset::Column::Delegate,
                    asset::Column::SlotUpdatedTokenAccount,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    query.sql = format!(
    "{} WHERE (excluded.slot_updated_token_account >= asset.slot_updated_token_account OR asset.slot_updated_token_account IS NULL)",
    query.sql);
    txn_or_conn.execute(query).await?;
    Ok(())
}

pub struct AssetMintAccountColumns {
    pub mint: Vec<u8>,
    pub supply: Decimal,
    pub slot_updated_mint_account: i64,
    pub extensions: Option<Value>,
}

pub async fn upsert_assets_mint_account_columns<T: ConnectionTrait + TransactionTrait>(
    columns: AssetMintAccountColumns,
    txn_or_conn: &T,
) -> Result<(), DbErr> {
    let active_model = asset::ActiveModel {
        id: Set(columns.mint.clone()),
        supply: Set(columns.supply),
        supply_mint: Set(Some(columns.mint.clone())),
        slot_updated_mint_account: Set(Some(columns.slot_updated_mint_account)),
        slot_updated: Set(Some(columns.slot_updated_mint_account)),
        mint_extensions: Set(columns.extensions),
        asset_data: Set(Some(columns.mint.clone())),
        // assume every token is a fungible token when mint account is created
        specification_asset_class: Set(Some(SpecificationAssetClass::FungibleToken)),
        // // assume multiple ownership as we set asset class to fungible token
        owner_type: Set(OwnerType::Token),
        ..Default::default()
    };
    let mut query = asset::Entity::insert(active_model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::Supply,
                    asset::Column::SupplyMint,
                    asset::Column::SlotUpdatedMintAccount,
                    asset::Column::MintExtensions,
                    asset::Column::SlotUpdated,
                    asset::Column::AssetData,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    query.sql = format!(
    "{} WHERE excluded.slot_updated_mint_account >= asset.slot_updated_mint_account OR asset.slot_updated_mint_account IS NULL",
    query.sql);
    txn_or_conn.execute(query).await?;
    Ok(())
}

pub struct AssetMetadataAccountColumns {
    pub mint: Vec<u8>,
    pub specification_asset_class: Option<SpecificationAssetClass>,
    pub owner_type: OwnerType,
    pub royalty_amount: i32,
    pub asset_data: Option<Vec<u8>>,
    pub slot_updated_metadata_account: u64,
    pub mpl_core_plugins: Option<Value>,
    pub mpl_core_unknown_plugins: Option<Value>,
    pub mpl_core_collection_num_minted: Option<i32>,
    pub mpl_core_collection_current_size: Option<i32>,
    pub mpl_core_plugins_json_version: Option<i32>,
    pub mpl_core_external_plugins: Option<Value>,
    pub mpl_core_unknown_external_plugins: Option<Value>,
}

pub async fn upsert_assets_metadata_account_columns<T: ConnectionTrait + TransactionTrait>(
    columns: AssetMetadataAccountColumns,
    txn_or_conn: &T,
) -> Result<(), DbErr> {
    let active_model = asset::ActiveModel {
        id: Set(columns.mint),
        specification_version: Set(Some(SpecificationVersions::V1)),
        specification_asset_class: Set(columns.specification_asset_class),
        owner_type: Set(columns.owner_type),
        tree_id: Set(None),
        nonce: Set(Some(0)),
        seq: Set(Some(0)),
        leaf: Set(None),
        data_hash: Set(None),
        creator_hash: Set(None),
        compressed: Set(false),
        compressible: Set(false),
        royalty_target_type: Set(RoyaltyTargetType::Creators),
        royalty_target: Set(None),
        royalty_amount: Set(columns.royalty_amount),
        asset_data: Set(columns.asset_data),
        slot_updated_metadata_account: Set(Some(columns.slot_updated_metadata_account as i64)),
        burnt: Set(false),
        mpl_core_plugins: Set(columns.mpl_core_plugins),
        mpl_core_unknown_plugins: Set(columns.mpl_core_unknown_plugins),
        mpl_core_collection_num_minted: Set(columns.mpl_core_collection_num_minted),
        mpl_core_collection_current_size: Set(columns.mpl_core_collection_current_size),
        mpl_core_plugins_json_version: Set(columns.mpl_core_plugins_json_version),
        mpl_core_external_plugins: Set(columns.mpl_core_external_plugins),
        mpl_core_unknown_external_plugins: Set(columns.mpl_core_unknown_external_plugins),
        ..Default::default()
    };
    let mut query = asset::Entity::insert(active_model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::SpecificationVersion,
                    asset::Column::SpecificationAssetClass,
                    asset::Column::OwnerType,
                    asset::Column::TreeId,
                    asset::Column::Nonce,
                    asset::Column::Seq,
                    asset::Column::Leaf,
                    asset::Column::DataHash,
                    asset::Column::CreatorHash,
                    asset::Column::Compressed,
                    asset::Column::Compressible,
                    asset::Column::RoyaltyTargetType,
                    asset::Column::RoyaltyTarget,
                    asset::Column::RoyaltyAmount,
                    asset::Column::AssetData,
                    asset::Column::SlotUpdatedMetadataAccount,
                    asset::Column::Burnt,
                    asset::Column::MplCorePlugins,
                    asset::Column::MplCoreUnknownPlugins,
                    asset::Column::MplCoreCollectionNumMinted,
                    asset::Column::MplCoreCollectionCurrentSize,
                    asset::Column::MplCorePluginsJsonVersion,
                    asset::Column::MplCoreExternalPlugins,
                    asset::Column::MplCoreUnknownExternalPlugins,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    query.sql = format!(
        "{} WHERE excluded.slot_updated_metadata_account >= asset.slot_updated_metadata_account OR asset.slot_updated_metadata_account IS NULL",
        query.sql);
    txn_or_conn.execute(query).await?;
    Ok(())
}
