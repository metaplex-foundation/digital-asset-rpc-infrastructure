use sea_orm::sea_query::{Alias, Expr, Order, Query, SelectStatement};
use sea_orm::{
    DeriveIden, EntityTrait, EnumIter, FromQueryResult, JoinType, Related, RelationDef,
    RelationTrait,
};

use crate::dao::{
    asset, asset_authority, asset_creators, asset_data, asset_grouping,
    asset_v1_account_attachments,
    sea_orm_active_enums::{
        ChainMutability, Mutability, OwnerType, RoyaltyTargetType, SpecificationAssetClass,
        SpecificationVersions,
    },
    token_accounts,
};
use crate::dao::{Cursor, Pagination};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, EnumIter, DeriveIden)]
pub enum Relation {
    AssetData,
    AssetV1AccountAttachments,
    AssetAuthority,
    AssetCreators,
    AssetGrouping,
    TokenAccounts,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::AssetData => asset::Entity::belongs_to(asset_data::Entity)
                .from(asset::Column::AssetData)
                .to(asset_data::Column::Id)
                .into(),
            Self::TokenAccounts => asset::Entity::has_many(token_accounts::Entity)
                .from(asset::Column::Id)
                .to(token_accounts::Column::Mint)
                .into(),
            Self::AssetV1AccountAttachments => {
                asset::Entity::has_many(asset_v1_account_attachments::Entity).into()
            }
            Self::AssetAuthority => asset::Entity::has_many(asset_authority::Entity).into(),
            Self::AssetCreators => asset::Entity::has_many(asset_creators::Entity).into(),
            Self::AssetGrouping => asset::Entity::has_many(asset_grouping::Entity).into(),
        }
    }
}

impl Related<asset_data::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetData.def()
    }
}

impl Related<asset_v1_account_attachments::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetV1AccountAttachments.def()
    }
}

impl Related<asset_authority::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetAuthority.def()
    }
}

impl Related<asset_creators::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetCreators.def()
    }
}

impl Related<asset_grouping::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::AssetGrouping.def()
    }
}

impl Related<token_accounts::Entity> for asset::Entity {
    fn to() -> RelationDef {
        Relation::TokenAccounts.def()
    }
}

impl Default for RoyaltyTargetType {
    fn default() -> Self {
        Self::Creators
    }
}

impl Default for asset::Model {
    fn default() -> Self {
        Self {
            id: vec![],
            alt_id: None,
            specification_version: None,
            specification_asset_class: None,
            owner: None,
            owner_type: OwnerType::Unknown,
            delegate: None,
            frozen: Default::default(),
            supply: Default::default(),
            supply_mint: None,
            compressed: Default::default(),
            compressible: Default::default(),
            seq: None,
            tree_id: None,
            leaf: None,
            nonce: None,
            royalty_target_type: RoyaltyTargetType::Unknown,
            royalty_target: None,
            royalty_amount: Default::default(),
            asset_data: None,
            created_at: None,
            burnt: Default::default(),
            slot_updated: None,
            slot_updated_metadata_account: None,
            slot_updated_mint_account: None,
            slot_updated_token_account: None,
            slot_updated_cnft_transaction: None,
            data_hash: None,
            creator_hash: None,
            owner_delegate_seq: None,
            leaf_seq: None,
            base_info_seq: None,
            mint_extensions: None,
            mpl_core_plugins: None,
            mpl_core_unknown_plugins: None,
            mpl_core_collection_current_size: None,
            mpl_core_collection_num_minted: None,
            mpl_core_plugins_json_version: None,
            mpl_core_external_plugins: None,
            mpl_core_unknown_external_plugins: None,
            collection_hash: None,
            asset_data_hash: None,
            bubblegum_flags: None,
            non_transferable: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, FromQueryResult)]
pub struct Row {
    // asset
    pub id: Vec<u8>,
    pub alt_id: Option<Vec<u8>>,
    pub specification_version: Option<SpecificationVersions>,
    pub specification_asset_class: Option<SpecificationAssetClass>,
    pub asset_owner: Option<Vec<u8>>,
    pub owner_type: OwnerType,
    pub asset_delegate: Option<Vec<u8>>,
    pub asset_frozen: bool,
    pub supply: Decimal,
    pub supply_mint: Option<Vec<u8>>,
    pub compressed: bool,
    pub compressible: bool,
    pub seq: Option<i64>,
    pub tree_id: Option<Vec<u8>>,
    pub leaf: Option<Vec<u8>>,
    pub nonce: Option<i64>,
    pub royalty_target_type: RoyaltyTargetType,
    pub royalty_target: Option<Vec<u8>>,
    pub royalty_amount: i32,
    pub created_at: Option<DateTimeWithTimeZone>,
    pub burnt: bool,
    pub slot_updated: Option<i64>,
    pub data_hash: Option<String>,
    pub creator_hash: Option<String>,
    pub mint_extensions: Option<Json>,
    pub mpl_core_plugins: Option<Json>,
    pub mpl_core_unknown_plugins: Option<Json>,
    pub mpl_core_collection_num_minted: Option<i32>,
    pub mpl_core_collection_current_size: Option<i32>,
    pub mpl_core_plugins_json_version: Option<i32>,
    pub mpl_core_external_plugins: Option<Json>,
    pub mpl_core_unknown_external_plugins: Option<Json>,
    pub collection_hash: Option<String>,
    pub asset_data_hash: Option<String>,
    pub bubblegum_flags: Option<i16>,
    pub non_transferable: Option<bool>,

    // asset_data
    pub chain_data_mutability: Option<ChainMutability>,
    pub chain_data: Option<Json>,
    pub metadata_url: Option<String>,
    pub metadata_mutability: Option<Mutability>,
    pub metadata: Option<Json>,
    pub raw_name: Option<Vec<u8>>,
    pub raw_symbol: Option<Vec<u8>>,

    // mint
    pub mint_supply: Option<Decimal>,
    pub mint_decimals: Option<i32>,
    pub mint_token_program: Option<Vec<u8>>,
    pub mint_authority: Option<Vec<u8>>,
    pub mint_freeze_authority: Option<Vec<u8>>,
    pub mint_close_authority: Option<Vec<u8>>,
    pub mint_extension_data: Option<Vec<u8>>,

    // token_account
    pub token_account_pubkey: Option<Vec<u8>>,
    pub token_owner: Option<Vec<u8>>,
    pub token_account_delegate: Option<Vec<u8>>,
    pub token_account_amount: Option<i64>,
    pub token_account_frozen: Option<bool>,
    pub token_account_close_authority: Option<Vec<u8>>,
    pub token_account_delegated_amount: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    // asset
    Id,
    AltId,
    SpecificationVersion,
    SpecificationAssetClass,
    AssetOwner,
    OwnerType,
    AssetDelegate,
    AssetFrozen,
    Supply,
    SupplyMint,
    Compressed,
    Compressible,
    Seq,
    TreeId,
    Leaf,
    Nonce,
    RoyaltyTargetType,
    RoyaltyTarget,
    RoyaltyAmount,
    CreatedAt,
    Burnt,
    SlotUpdated,
    DataHash,
    CreatorHash,
    MintExtensions,
    MplCorePlugins,
    MplCoreUnknownPlugins,
    MplCoreCollectionNumMinted,
    MplCoreCollectionCurrentSize,
    MplCorePluginsJsonVersion,
    MplCoreExternalPlugins,
    MplCoreUnknownExternalPlugins,
    CollectionHash,
    AssetDataHash,
    BubblegumFlags,
    NonTransferable,

    // asset_data
    ChainDataMutability,
    ChainData,
    MetadataUrl,
    MetadataMutability,
    Metadata,
    RawName,
    RawSymbol,

    // mint
    MintSupply,
    MintDecimals,
    MintTokenProgram,
    MintAuthority,
    MintFreezeAuthority,
    MintCloseAuthority,
    MintExtensionData,

    // token_account
    TokenAccountPubkey,
    TokenOwner,
    TokenAccountDelegate,
    TokenAccountAmount,
    TokenAccountFrozen,
    TokenAccountCloseAuthority,
    TokenAccountDelegatedAmount,
}

impl Default for Row {
    fn default() -> Self {
        Self {
            id: vec![],
            alt_id: None,
            specification_version: None,
            specification_asset_class: None,
            asset_owner: None,
            asset_delegate: None,
            asset_frozen: false,
            owner_type: OwnerType::Unknown,
            supply: Decimal::new(0, 0),
            supply_mint: None,
            compressed: false,
            compressible: false,
            seq: None,
            tree_id: None,
            leaf: None,
            nonce: None,
            royalty_target_type: RoyaltyTargetType::Unknown,
            royalty_target: None,
            royalty_amount: 0,
            created_at: None,
            burnt: false,
            slot_updated: None,
            data_hash: None,
            creator_hash: None,
            mint_extensions: None,
            mpl_core_plugins: None,
            mpl_core_unknown_plugins: None,
            mpl_core_collection_num_minted: None,
            mpl_core_collection_current_size: None,
            mpl_core_plugins_json_version: None,
            mpl_core_external_plugins: None,
            mpl_core_unknown_external_plugins: None,
            collection_hash: None,
            asset_data_hash: None,
            bubblegum_flags: None,
            non_transferable: None,
            chain_data_mutability: None,
            chain_data: None,
            metadata_url: None,
            metadata_mutability: None,
            metadata: None,
            raw_name: None,
            raw_symbol: None,
            mint_supply: None,
            mint_decimals: None,
            mint_token_program: None,
            mint_authority: None,
            mint_freeze_authority: None,
            mint_close_authority: None,
            mint_extension_data: None,
            token_account_pubkey: None,
            token_owner: None,
            token_account_delegate: None,
            token_account_amount: None,
            token_account_frozen: None,
            token_account_close_authority: None,
            token_account_delegated_amount: None,
        }
    }
}

impl Row {
    pub fn select() -> SelectStatement {
        Query::select()
            .column((Entity, asset::Column::Id))
            .column((Entity, asset::Column::AltId))
            .expr(
                Expr::col((Entity, asset::Column::SpecificationVersion))
                    .as_enum(Alias::new("TEXT")),
            )
            .expr(
                Expr::col((Entity, asset::Column::SpecificationAssetClass))
                    .as_enum(Alias::new("TEXT")),
            )
            .expr_as(
                Expr::col((Entity, asset::Column::Owner)),
                Column::AssetOwner,
            )
            .expr(Expr::col((Entity, asset::Column::OwnerType)).as_enum(Alias::new("TEXT")))
            .expr_as(
                Expr::col((Entity, asset::Column::Delegate)),
                Column::AssetDelegate,
            )
            .expr_as(
                Expr::col((Entity, asset::Column::Frozen)),
                Column::AssetFrozen,
            )
            .column((Entity, asset::Column::Supply))
            .column((Entity, asset::Column::SupplyMint))
            .column((Entity, asset::Column::Compressed))
            .column((Entity, asset::Column::Compressible))
            .column((Entity, asset::Column::Seq))
            .column((Entity, asset::Column::TreeId))
            .column((Entity, asset::Column::Leaf))
            .column((Entity, asset::Column::Nonce))
            .expr(Expr::col((Entity, asset::Column::RoyaltyTargetType)).as_enum(Alias::new("TEXT")))
            .column((Entity, asset::Column::RoyaltyTarget))
            .column((Entity, asset::Column::RoyaltyAmount))
            .column((Entity, asset::Column::AssetData))
            .column((Entity, asset::Column::CreatedAt))
            .column((Entity, asset::Column::Burnt))
            .column((Entity, asset::Column::SlotUpdated))
            .column((Entity, asset::Column::DataHash))
            .column((Entity, asset::Column::CreatorHash))
            .column((Entity, asset::Column::MintExtensions))
            .column((Entity, asset::Column::MplCorePlugins))
            .column((Entity, asset::Column::MplCoreUnknownPlugins))
            .column((Entity, asset::Column::MplCoreCollectionNumMinted))
            .column((Entity, asset::Column::MplCoreCollectionCurrentSize))
            .column((Entity, asset::Column::MplCorePluginsJsonVersion))
            .column((Entity, asset::Column::MplCoreExternalPlugins))
            .column((Entity, asset::Column::MplCoreUnknownExternalPlugins))
            .column((Entity, asset::Column::CollectionHash))
            .column((Entity, asset::Column::AssetDataHash))
            .column((Entity, asset::Column::BubblegumFlags))
            .column((Entity, asset::Column::NonTransferable))
            .expr(
                Expr::col((asset_data::Entity, asset_data::Column::ChainDataMutability))
                    .as_enum(Alias::new("TEXT")),
            )
            .column((asset_data::Entity, asset_data::Column::ChainData))
            .column((asset_data::Entity, asset_data::Column::MetadataUrl))
            .expr(
                Expr::col((asset_data::Entity, asset_data::Column::MetadataMutability))
                    .as_enum(Alias::new("TEXT")),
            )
            .column((asset_data::Entity, asset_data::Column::Metadata))
            .column((asset_data::Entity, asset_data::Column::RawName))
            .column((asset_data::Entity, asset_data::Column::RawSymbol))
            .join(
                JoinType::LeftJoin,
                asset_data::Entity,
                Expr::tbl(Entity, asset::Column::Id)
                    .equals(asset_data::Entity, asset_data::Column::Id),
            )
            .to_owned()
    }
}

#[derive(Copy, Clone, Default, Debug)]
pub struct Entity;

impl Iden for Entity {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "extension_assets").unwrap();
    }
}

impl IdenStatic for Entity {
    fn as_str(&self) -> &str {
        "extension_assets"
    }
}

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "extension_assets"
    }
}
pub trait AssetSelectStatementExt {
    fn sort_by(self, column: Column, direction: &Order) -> Self;

    fn page_by(
        self,
        pagination: &Pagination,
        limit: u64,
        sort_direction: &Order,
        col: Column,
    ) -> Self;
}

impl AssetSelectStatementExt for SelectStatement {
    fn sort_by(mut self, col: Column, direction: &Order) -> Self {
        self.order_by_expr(Expr::tbl(Entity, col).into(), direction.clone())
            .order_by_expr(Expr::tbl(Entity, Column::Id).into(), Order::Asc)
            .to_owned()
    }

    fn page_by(
        mut self,
        pagination: &Pagination,
        limit: u64,
        order: &Order,
        column: Column,
    ) -> Self {
        match pagination {
            Pagination::Keyset { before, after } => {
                if let Some(b) = before {
                    self = self
                        .and_where(Expr::tbl(Entity, column).lt(b.clone()))
                        .to_owned();
                }
                if let Some(a) = after {
                    self = self
                        .and_where(Expr::tbl(Entity, column).gt(a.clone()))
                        .to_owned();
                }
            }
            Pagination::Page { page } => {
                if *page > 0 {
                    self = self.offset((page - 1) * limit).to_owned();
                }
            }
            Pagination::Cursor(cursor) => {
                if *cursor != Cursor::default() {
                    if order == &Order::Asc {
                        self = self
                            .and_where(Expr::tbl(Entity, column).gt(cursor.id.clone()))
                            .to_owned();
                    } else {
                        self = self
                            .and_where(Expr::tbl(Entity, column).lt(cursor.id.clone()))
                            .to_owned();
                    }
                }
            }
        }
        self.limit(limit).to_owned()
    }
}
