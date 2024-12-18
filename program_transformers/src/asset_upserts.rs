use {
    digital_asset_types::dao::{
        asset,
        sea_orm_active_enums::{
            OwnerType, RoyaltyTargetType, SpecificationAssetClass, SpecificationVersions,
        },
    },
    sea_orm::{
        sea_query::{Alias, Expr, OnConflict},
        Condition, ConnectionTrait, DbErr, EntityTrait, Set, TransactionTrait,
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
    asset::Entity::insert(active_model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::Owner,
                    asset::Column::Frozen,
                    asset::Column::Delegate,
                    asset::Column::SlotUpdatedTokenAccount,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset::Column::Owner)
                                        .ne(Expr::tbl(asset::Entity, asset::Column::Owner)),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset::Column::Frozen)
                                        .ne(Expr::tbl(asset::Entity, asset::Column::Frozen)),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset::Column::Delegate)
                                        .ne(Expr::tbl(asset::Entity, asset::Column::Delegate)),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::SlotUpdatedTokenAccount,
                                    )
                                    .ne(Expr::tbl(
                                        asset::Entity,
                                        asset::Column::SlotUpdatedTokenAccount,
                                    )),
                                ),
                        )
                        .add_option(columns.slot_updated_token_account.map(|slot| {
                            Expr::tbl(asset::Entity, asset::Column::SlotUpdatedTokenAccount)
                                .lte(slot)
                        })),
                )
                .to_owned(),
        )
        .exec_without_returning(txn_or_conn)
        .await?;

    Ok(())
}

pub struct AssetMintAccountColumns {
    pub mint: Vec<u8>,
    pub supply: Decimal,
    pub supply_mint: Option<Vec<u8>>,
    pub slot_updated_mint_account: u64,
}

pub async fn upsert_assets_mint_account_columns<T: ConnectionTrait + TransactionTrait>(
    columns: AssetMintAccountColumns,
    txn_or_conn: &T,
) -> Result<(), DbErr> {
    let owner_type = if columns.supply == Decimal::from(1) {
        OwnerType::Single
    } else {
        OwnerType::Token
    };

    let active_model = asset::ActiveModel {
        id: Set(columns.mint),
        supply: Set(columns.supply),
        supply_mint: Set(columns.supply_mint),
        slot_updated_mint_account: Set(Some(columns.slot_updated_mint_account as i64)),
        owner_type: Set(owner_type),
        ..Default::default()
    };

    asset::Entity::insert(active_model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::Supply,
                    asset::Column::SupplyMint,
                    asset::Column::SlotUpdatedMintAccount,
                    asset::Column::OwnerType,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset::Column::Supply)
                                        .ne(Expr::tbl(asset::Entity, asset::Column::Supply)),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset::Column::SupplyMint)
                                        .ne(Expr::tbl(asset::Entity, asset::Column::SupplyMint)),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset::Column::OwnerType)
                                        .ne(Expr::tbl(asset::Entity, asset::Column::OwnerType)),
                                ),
                        )
                        .add(
                            Expr::tbl(asset::Entity, asset::Column::SlotUpdatedMintAccount)
                                .lte(columns.slot_updated_mint_account as i64),
                        ),
                )
                .to_owned(),
        )
        .exec_without_returning(txn_or_conn)
        .await?;
    Ok(())
}

pub struct AssetMetadataAccountColumns {
    pub mint: Vec<u8>,
    pub specification_asset_class: Option<SpecificationAssetClass>,
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

    asset::Entity::insert(active_model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::SpecificationAssetClass,
                    asset::Column::RoyaltyAmount,
                    asset::Column::AssetData,
                    asset::Column::SlotUpdatedMetadataAccount,
                    asset::Column::MplCorePlugins,
                    asset::Column::MplCoreUnknownPlugins,
                    asset::Column::MplCoreCollectionNumMinted,
                    asset::Column::MplCoreCollectionCurrentSize,
                    asset::Column::MplCorePluginsJsonVersion,
                    asset::Column::MplCoreExternalPlugins,
                    asset::Column::MplCoreUnknownExternalPlugins,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::SpecificationAssetClass,
                                    )
                                    .ne(Expr::tbl(
                                        asset::Entity,
                                        asset::Column::SpecificationAssetClass,
                                    )),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset::Column::RoyaltyAmount)
                                        .ne(Expr::tbl(asset::Entity, asset::Column::RoyaltyAmount)),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset::Column::AssetData)
                                        .ne(Expr::tbl(asset::Entity, asset::Column::AssetData)),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::SlotUpdatedMetadataAccount,
                                    )
                                    .ne(Expr::tbl(
                                        asset::Entity,
                                        asset::Column::SlotUpdatedMetadataAccount,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::MplCorePlugins,
                                    )
                                    .ne(Expr::tbl(asset::Entity, asset::Column::MplCorePlugins)),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::MplCoreUnknownPlugins,
                                    )
                                    .ne(Expr::tbl(
                                        asset::Entity,
                                        asset::Column::MplCoreUnknownPlugins,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::MplCoreCollectionNumMinted,
                                    )
                                    .ne(Expr::tbl(
                                        asset::Entity,
                                        asset::Column::MplCoreCollectionNumMinted,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::MplCoreCollectionCurrentSize,
                                    )
                                    .ne(Expr::tbl(
                                        asset::Entity,
                                        asset::Column::MplCoreCollectionCurrentSize,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::MplCorePluginsJsonVersion,
                                    )
                                    .ne(Expr::tbl(
                                        asset::Entity,
                                        asset::Column::MplCorePluginsJsonVersion,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::MplCoreExternalPlugins,
                                    )
                                    .ne(Expr::tbl(
                                        asset::Entity,
                                        asset::Column::MplCoreExternalPlugins,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset::Column::MplCoreUnknownExternalPlugins,
                                    )
                                    .ne(Expr::tbl(
                                        asset::Entity,
                                        asset::Column::MplCoreUnknownExternalPlugins,
                                    )),
                                ),
                        )
                        .add(
                            Expr::tbl(asset::Entity, asset::Column::SlotUpdatedMetadataAccount)
                                .lte(columns.slot_updated_metadata_account as i64),
                        ),
                )
                .to_owned(),
        )
        .exec_without_returning(txn_or_conn)
        .await?;

    Ok(())
}
