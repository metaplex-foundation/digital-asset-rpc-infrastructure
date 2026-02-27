use schemars::JsonSchema;

use {
    crate::rpc::{Asset, Interface, TokenAccount},
    serde::{Deserialize, Serialize},
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub enum AssetCategory {
    NFT,
    FungibleAsset,
    FungibleToken,
    Custom,
}

impl AssetCategory {
    pub fn to_asset_classes(&self) -> Vec<&'static str> {
        match self {
            AssetCategory::NFT => vec![
                "NFT",
                "PRINTABLE_NFT",
                "PRINT",
                "PROGRAMMABLE_NFT",
                "NON_TRANSFERABLE_NFT",
                "TRANSFER_RESTRICTED_NFT",
                "IDENTITY_NFT",
                "MPL_CORE_ASSET",
                "MPL_CORE_COLLECTION",
                "MPL_BUBBLEGUM_V2",
            ],
            AssetCategory::FungibleAsset => vec!["FUNGIBLE_ASSET"],
            AssetCategory::FungibleToken => vec!["FUNGIBLE_TOKEN"],
            AssetCategory::Custom => vec!["unknown"],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AssetChangeItem {
    pub id: String,
    #[serde(rename = "type")]
    pub interface: Interface,
    pub slot_updated: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegate: Option<String>,
    pub burnt: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AssetChangeList {
    pub current_slot: i64,
    pub items: Vec<AssetChangeItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct DasError {
    pub id: String,
    pub error: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct GetGroupingResponse {
    pub group_key: String,
    pub group_name: String,
    pub group_size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct AssetList {
    pub total: u32,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub items: Vec<Asset>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<DasError>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct TransactionSignatureList {
    pub total: u32,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    pub items: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct TokenAccountList {
    pub total: u32,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    pub token_accounts: Vec<TokenAccount>,
    pub cursor: Option<String>,
    pub errors: Vec<DasError>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]

pub struct NftEdition {
    pub mint_address: String,
    pub edition_address: String,
    pub edition_number: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct NftEditions {
    pub total: u32,
    pub limit: u32,
    pub master_edition_address: String,
    pub supply: u64,
    pub max_supply: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub editions: Vec<NftEdition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}
