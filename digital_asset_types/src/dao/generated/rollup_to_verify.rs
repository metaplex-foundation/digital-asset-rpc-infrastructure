//! SeaORM Entity. Generated by sea-orm-codegen 0.9.3

use super::sea_orm_active_enums::RollupFailStatus;
use super::sea_orm_active_enums::RollupPersistingState;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "rollup_to_verify"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Serialize, Deserialize)]
pub struct Model {
    pub file_hash: String,
    pub url: String,
    pub created_at_slot: i64,
    pub signature: Vec<u8>,
    pub download_attempts: i32,
    pub rollup_persisting_state: RollupPersistingState,
    pub rollup_fail_status: Option<RollupFailStatus>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    FileHash,
    Url,
    CreatedAtSlot,
    Signature,
    DownloadAttempts,
    RollupPersistingState,
    RollupFailStatus,
}

#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {
    FileHash,
}

impl PrimaryKeyTrait for PrimaryKey {
    type ValueType = String;
    fn auto_increment() -> bool {
        false
    }
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            Self::FileHash => ColumnType::String(None).def(),
            Self::Url => ColumnType::String(None).def(),
            Self::CreatedAtSlot => ColumnType::BigInteger.def(),
            Self::Signature => ColumnType::Binary.def(),
            Self::DownloadAttempts => ColumnType::Integer.def(),
            Self::RollupPersistingState => RollupPersistingState::db_type(),
            Self::RollupFailStatus => RollupFailStatus::db_type().null(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        panic!("No RelationDef")
    }
}

impl ActiveModelBehavior for ActiveModel {}
