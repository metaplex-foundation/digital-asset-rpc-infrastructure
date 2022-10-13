//! SeaORM Entity. Generated by sea-orm-codegen 0.9.2

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "candy_guard_group"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Serialize, Deserialize)]
pub struct Model {
    pub id: i64,
    pub label: Option<String>,
    pub candy_guard_id: Vec<u8>,
    pub gatekeeper_network: Option<Vec<u8>>,
    pub gatekeeper_expire_on_use: Option<bool>,
    pub allow_list_merkle_root: Option<Vec<u8>>,
    pub third_party_signer_key: Option<Vec<u8>>,
    pub mint_limit_id: Option<u8>,
    pub mint_limit_limit: Option<u16>,
    pub nft_payment_destination: Option<Vec<u8>>,
    pub nft_payment_required_collection: Option<Vec<u8>>,
    pub bot_tax_lamports: Option<i64>,
    pub bot_tax_last_instruction: Option<bool>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id,
    Label,
    CandyGuardId,
    GatekeeperNetwork,
    GatekeeperExpireOnUse,
    AllowListMerkleRoot,
    ThirdPartySignerKey,
    MintLimitId,
    MintLimitLimit,
    NftPaymentDestination,
    NftPaymentRequiredCollection,
    BotTaxLamports,
    BotTaxLastInstruction,
}

#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {
    Id,
}

impl PrimaryKeyTrait for PrimaryKey {
    type ValueType = i64;
    fn auto_increment() -> bool {
        true
    }
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    CandyGuard,
}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            Self::Id => ColumnType::BigInteger.def(),
            Self::Label => ColumnType::Text.def().null(),
            Self::CandyGuardId => ColumnType::Binary.def(),
            Self::GatekeeperNetwork => ColumnType::Binary.def().null(),
            Self::GatekeeperExpireOnUse => ColumnType::Boolean.def().null(),
            Self::AllowListMerkleRoot => ColumnType::Binary.def().null(),
            Self::ThirdPartySignerKey => ColumnType::Binary.def().null(),
            Self::MintLimitId => ColumnType::Integer.def().null(),
            Self::MintLimitLimit => ColumnType::Integer.def().null(),
            Self::NftPaymentDestination => ColumnType::Binary.def().null(),
            Self::NftPaymentRequiredCollection => ColumnType::Binary.def().null(),
            Self::BotTaxLamports => ColumnType::Integer.def().null(),
            Self::BotTaxLastInstruction => ColumnType::Boolean.def().null(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::CandyGuard => Entity::belongs_to(super::candy_guard::Entity)
                .from(Column::CandyGuardId)
                .to(super::candy_guard::Column::Id)
                .into(),
        }
    }
}

impl Related<super::candy_guard::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CandyGuard.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}