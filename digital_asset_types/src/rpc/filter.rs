use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]

pub struct AssetSorting {
    pub sort_by: AssetSortBy,
    pub sort_direction: AssetSortDirection,
}

impl Default for AssetSorting {
    fn default() -> AssetSorting {
        AssetSorting {
            sort_by: AssetSortBy::Created,
            sort_direction: AssetSortDirection::Desc,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]

pub enum AssetSortBy {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "updated")]
    Updated,
    #[serde(rename = "recent_action")]
    RecentAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum AssetSortDirection {
    #[serde(rename = "asc")]
    Asc,
    #[serde(rename = "desc")]
    Desc,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub enum SearchConditionType {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "any")]
    Any,
}
