use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssetSorting {
    pub sort_by: AssetSortBy,
    pub sort_direction: Option<AssetSortDirection>,
}

impl Default for AssetSorting {
    fn default() -> AssetSorting {
        AssetSorting {
            sort_by: AssetSortBy::Id,
            sort_direction: Some(AssetSortDirection::default()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum AssetSortBy {
    #[serde(rename = "id")]
    Id,
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "updated")]
    Updated,
    #[serde(rename = "recent_action")]
    RecentAction,
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, Serialize, Deserialize, JsonSchema)]
pub enum TokenTypeClass {
    #[serde(rename = "fungible")]
    Fungible,
    #[serde(rename = "nonFungible")]
    NonFungible,
    #[serde(rename = "regularNFT")]
    Nft,
    #[serde(rename = "compressedNFT")]
    Compressed,
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum AssetSortDirection {
    #[default]
    #[serde(rename = "asc")]
    Asc,
    #[serde(rename = "desc")]
    Desc,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq, JsonSchema)]
pub enum SearchConditionType {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "any")]
    Any,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq, JsonSchema, Default)]
pub enum CommitmentLevel {
    #[serde(rename = "confirmed")]
    Confirmed,
    #[serde(rename = "finalized")]
    #[default]
    Finalized,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq, JsonSchema)]
pub struct CommitmentConfig {
    commitment: CommitmentLevel,
}
