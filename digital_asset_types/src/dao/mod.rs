#![allow(ambiguous_glob_reexports)]
mod full_asset;
mod generated;
pub mod scopes;
use crate::rpc::{filter::TokenTypeClass, Interface};

use self::sea_orm_active_enums::{
    OwnerType, RoyaltyTargetType, SpecificationAssetClass, SpecificationVersions,
};
pub use full_asset::*;
pub use generated::*;
pub mod extensions;

use sea_orm::{sea_query::ConditionType, DbErr};
use serde::{Deserialize, Serialize};

pub struct GroupingSize {
    pub size: u64,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PageOptions {
    pub limit: u64,
    pub page: Option<u64>,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
    pub cursor: Option<Cursor>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct Cursor {
    pub id: Option<Vec<u8>>,
}

pub enum Pagination {
    Keyset {
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
    },
    Page {
        page: u64,
    },
    Cursor(Cursor),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchAssetsQuery {
    // Conditions
    pub negate: Option<bool>,
    /// Defaults to [ConditionType::All]
    pub condition_type: Option<ConditionType>,
    pub interface: Option<Interface>,
    pub specification_version: Option<SpecificationVersions>,
    pub specification_asset_class: Option<SpecificationAssetClass>,
    pub owner_address: Option<Vec<u8>>,
    pub owner_type: Option<OwnerType>,
    pub creator_address: Option<Vec<u8>>,
    pub creator_verified: Option<bool>,
    pub authority_address: Option<Vec<u8>>,
    pub grouping: Option<(String, String)>,
    pub delegate: Option<Vec<u8>>,
    pub frozen: Option<bool>,
    pub supply: Option<u64>,
    pub supply_mint: Option<Vec<u8>>,
    pub compressed: Option<bool>,
    pub compressible: Option<bool>,
    pub royalty_target_type: Option<RoyaltyTargetType>,
    pub royalty_target: Option<Vec<u8>>,
    pub royalty_amount: Option<u32>,
    pub burnt: Option<bool>,
    pub json_uri: Option<String>,
    pub name: Option<Vec<u8>>,
    pub token_type: Option<TokenTypeClass>,
}

impl SearchAssetsQuery {
    pub fn validate(&self) -> Result<(), DbErr> {
        if self.token_type.is_some() {
            if self.owner_type.is_some() {
                return Err(DbErr::Custom(
                    "`ownerType` is not supported when using `tokenType` field".to_string(),
                ));
            }
            if self.owner_address.is_none() {
                return Err(DbErr::Custom(
                    "Must provide `ownerAddress` when using `tokenType` field".to_string(),
                ));
            }
            if self.interface.is_some() {
                return Err(DbErr::Custom(
                    "`interface` is not supported when using `tokenType` field".to_string(),
                ));
            }
        }
        Ok(())
    }
}
