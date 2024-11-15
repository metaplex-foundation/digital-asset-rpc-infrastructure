use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sea_orm::entity::prelude::*;

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

#[derive(Debug, Clone, PartialEq, Eq ,EnumIter, DeriveActiveEnum, Serialize, Deserialize,JsonSchema)]
#[sea_orm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "specification_asset_class"
)]
pub enum TokenTypeClass {
    #[sea_orm(string_value = "FUNGIBLE_ASSET")]
    FungibleAsset,
    #[sea_orm(string_value = "FUNGIBLE_TOKEN")]
    FungibleToken,
    // #[sea_orm(ignore)]
    #[sea_orm(string_value = "NON_FUNGIBLE_ASSET")]
    NonFungibleAsset,
    #[sea_orm(string_value = "MPL_CORE_ASSET")]
    MplCoreAsset,
    #[sea_orm(string_value = "MPL_CORE_COLLECTION")]
    MplCoreCollection,
    #[sea_orm(string_value = "NFT")]
    Nft,
    #[sea_orm(string_value = "PROGRAMMABLE_NFT")]
    ProgrammableNft,
    // #[sea_orm(ignore)]
    #[sea_orm(string_value = "COMPRESSED_NFT")]
    CompressedNft,
    // #[sea_orm(ignore)]
    #[sea_orm(string_value = "ALL")]
    All,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum AssetSortDirection {
    #[serde(rename = "asc")]
    Asc,
    #[serde(rename = "desc")]
    #[default]
    Desc,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq, JsonSchema)]
pub enum SearchConditionType {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "any")]
    Any,
}
