//! SeaORM Entity. Generated by sea-orm-codegen 0.9.3

use super::sea_orm_active_enums::ChainMutability;
use super::sea_orm_active_enums::Mutability;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "asset_data"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Serialize, Deserialize)]
pub struct Model {
    pub id: Vec<u8>,
    pub chain_data_mutability: ChainMutability,
    pub chain_data: Json,
    pub metadata_url: String,
    pub metadata_mutability: Mutability,
    pub metadata: Json,
    pub slot_updated: i64,
    pub reindex: Option<bool>,
    pub raw_name: Option<Vec<u8>>,
    pub raw_symbol: Option<Vec<u8>>,
    pub download_metadata_seq: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id,
    ChainDataMutability,
    ChainData,
    MetadataUrl,
    MetadataMutability,
    Metadata,
    SlotUpdated,
    Reindex,
    RawName,
    RawSymbol,
    DownloadMetadataSeq,
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
    Asset,
}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            Self::Id => ColumnType::Binary.def(),
            Self::ChainDataMutability => ChainMutability::db_type(),
            Self::ChainData => ColumnType::JsonBinary.def(),
            Self::MetadataUrl => ColumnType::String(Some(200u32)).def(),
            Self::MetadataMutability => Mutability::db_type(),
            Self::Metadata => ColumnType::JsonBinary.def(),
            Self::SlotUpdated => ColumnType::BigInteger.def(),
            Self::Reindex => ColumnType::Boolean.def().null(),
            Self::RawName => ColumnType::Binary.def().null(),
            Self::RawSymbol => ColumnType::Binary.def().null(),
            Self::DownloadMetadataSeq => ColumnType::BigInteger.def().null(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Asset => Entity::has_many(super::asset::Entity).into(),
        }
    }
}

impl Related<super::asset::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
