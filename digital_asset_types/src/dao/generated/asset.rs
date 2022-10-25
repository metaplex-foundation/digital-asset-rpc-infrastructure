//! SeaORM Entity. Generated by sea-orm-codegen 0.9.3

use super::sea_orm_active_enums::OwnerType;
use super::sea_orm_active_enums::RoyaltyTargetType;
use super::sea_orm_active_enums::SpecificationAssetClass;
use super::sea_orm_active_enums::SpecificationVersions;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "asset"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Serialize, Deserialize)]
pub struct Model {
    pub id: Vec<u8>,
    pub alt_id: Option<Vec<u8>>,
    pub specification_version: SpecificationVersions,
    pub specification_asset_class: SpecificationAssetClass,
    pub owner: Option<Vec<u8>>,
    pub owner_type: OwnerType,
    pub delegate: Option<Vec<u8>>,
    pub frozen: bool,
    pub supply: i64,
    pub supply_mint: Option<Vec<u8>>,
    pub compressed: bool,
    pub compressible: bool,
    pub seq: i64,
    pub tree_id: Option<Vec<u8>>,
    pub leaf: Option<Vec<u8>>,
    pub nonce: i64,
    pub royalty_target_type: RoyaltyTargetType,
    pub royalty_target: Option<Vec<u8>>,
    pub royalty_amount: i32,
    pub asset_data: Option<Vec<u8>>,
    pub created_at: Option<DateTimeWithTimeZone>,
    pub burnt: bool,
    pub slot_updated: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id,
    AltId,
    SpecificationVersion,
    SpecificationAssetClass,
    Owner,
    OwnerType,
    Delegate,
    Frozen,
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
    AssetData,
    CreatedAt,
    Burnt,
    SlotUpdated,
}

#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {
    Id,
}

impl PrimaryKeyTrait for PrimaryKey {
    type ValueType = Vec<u8>;
    fn auto_increment() -> bool {
        false
    }
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    AssetData,
    AssetV1AccountAttachments,
    AssetGrouping,
    AssetAuthority,
    AssetCreators,
}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            Self::Id => ColumnType::Binary.def(),
            Self::AltId => ColumnType::Binary.def().null(),
            Self::SpecificationVersion => SpecificationVersions::db_type(),
            Self::SpecificationAssetClass => SpecificationAssetClass::db_type(),
            Self::Owner => ColumnType::Binary.def().null(),
            Self::OwnerType => OwnerType::db_type(),
            Self::Delegate => ColumnType::Binary.def().null(),
            Self::Frozen => ColumnType::Boolean.def(),
            Self::Supply => ColumnType::BigInteger.def(),
            Self::SupplyMint => ColumnType::Binary.def().null(),
            Self::Compressed => ColumnType::Boolean.def(),
            Self::Compressible => ColumnType::Boolean.def(),
            Self::Seq => ColumnType::BigInteger.def(),
            Self::TreeId => ColumnType::Binary.def().null(),
            Self::Leaf => ColumnType::Binary.def().null(),
            Self::Nonce => ColumnType::BigInteger.def(),
            Self::RoyaltyTargetType => RoyaltyTargetType::db_type(),
            Self::RoyaltyTarget => ColumnType::Binary.def().null(),
            Self::RoyaltyAmount => ColumnType::Integer.def(),
            Self::AssetData => ColumnType::Binary.def().null(),
            Self::CreatedAt => ColumnType::TimestampWithTimeZone.def().null(),
            Self::Burnt => ColumnType::Boolean.def(),
            Self::SlotUpdated => ColumnType::BigInteger.def(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::AssetData => Entity::belongs_to(super::asset_data::Entity)
                .from(Column::AssetData)
                .to(super::asset_data::Column::Id)
                .into(),
            Self::AssetV1AccountAttachments => {
                Entity::has_many(super::asset_v1_account_attachments::Entity).into()
            }
            Self::AssetGrouping => Entity::has_many(super::asset_grouping::Entity).into(),
            Self::AssetAuthority => Entity::has_many(super::asset_authority::Entity).into(),
            Self::AssetCreators => Entity::has_many(super::asset_creators::Entity).into(),
        }
    }
}

impl Related<super::asset_data::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AssetData.def()
    }
}

impl Related<super::asset_v1_account_attachments::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AssetV1AccountAttachments.def()
    }
}

impl Related<super::asset_grouping::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AssetGrouping.def()
    }
}

impl Related<super::asset_authority::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AssetAuthority.def()
    }
}

impl Related<super::asset_creators::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AssetCreators.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
