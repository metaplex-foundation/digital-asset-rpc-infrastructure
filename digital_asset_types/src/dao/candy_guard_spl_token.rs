//! SeaORM Entity. Generated by sea-orm-codegen 0.9.2

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "candy_guard_spl_token"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Serialize, Deserialize)]
pub struct Model {
    pub id: u8,
    pub amount: u64,
    pub token_mint: Vec<u8>,
    pub destination_ata: Vec<u8>,
    pub candy_guard_id: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id,
    Amount,
    TokenMint,
    DestinationAta,
    CandyGuardId,
}

#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {
    Id,
}

impl PrimaryKeyTrait for PrimaryKey {
    type ValueType = Vec<u8>;
    fn auto_increment() -> bool {
        true
    }
}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            Self::Id => ColumnType::BigInteger.def(),
            Self::Amount => ColumnType::Integer.def(),
            Self::TokenMint => ColumnType::Binary.def(),
            Self::DestinationAta => ColumnType::Binary.def(),
            Self::CandyGuardId => ColumnType::BigInteger.def(),
        }
    }
}

impl ActiveModelBehavior for ActiveModel {}