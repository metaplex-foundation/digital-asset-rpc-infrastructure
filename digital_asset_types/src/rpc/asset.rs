use crate::dao::sea_orm_active_enums::{
    OwnerType, RoyaltyTargetType, SpecificationAssetClass, SpecificationVersions,
};
#[cfg(feature = "sql_types")]
use std::collections::BTreeMap;

use crate::dao::sea_orm_active_enums::ChainMutability;
use schemars::JsonSchema;
use serde_json::Value;
use {
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AssetProof {
    pub root: String,
    pub proof: Vec<String>,
    pub node_index: i64,
    pub leaf: String,
    pub tree_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema, Default)]
pub enum Interface {
    #[serde(rename = "V1_NFT")]
    V1NFT,
    #[serde(rename = "V1_PRINT")]
    V1PRINT,
    #[serde(rename = "V2_NFT")]
    Nft,
    // TODO: change on version bump
    #[serde(rename = "LEGACY_NFT")]
    #[allow(non_camel_case_types)]
    LEGACY_NFT,
    #[serde(rename = "FungibleAsset")]
    FungibleAsset,
    #[serde(rename = "FungibleToken")]
    FungibleToken,
    #[serde(rename = "Identity")]
    Identity,
    #[serde(rename = "Executable")]
    Executable,
    #[serde(rename = "ProgrammableNFT")]
    ProgrammableNFT,
    #[serde(rename = "MplCoreAsset")]
    MplCoreAsset,
    #[serde(rename = "MplCoreCollection")]
    MplCoreCollection,
    #[default]
    #[serde(rename = "Custom")]
    Custom,
}

impl From<(Option<&SpecificationVersions>, &SpecificationAssetClass)> for Interface {
    fn from(i: (Option<&SpecificationVersions>, &SpecificationAssetClass)) -> Self {
        match i {
            (Some(SpecificationVersions::V1), SpecificationAssetClass::Nft) => Interface::V1NFT,
            (Some(SpecificationVersions::V1), SpecificationAssetClass::PrintableNft) => {
                Interface::V1NFT
            }
            (Some(SpecificationVersions::V0), SpecificationAssetClass::Nft) => {
                Interface::LEGACY_NFT
            }
            (Some(SpecificationVersions::V1), SpecificationAssetClass::ProgrammableNft) => {
                Interface::ProgrammableNFT
            }
            (_, SpecificationAssetClass::MplCoreAsset) => Interface::MplCoreAsset,
            (_, SpecificationAssetClass::MplCoreCollection) => Interface::MplCoreCollection,
            (_, SpecificationAssetClass::FungibleAsset) => Interface::FungibleAsset,
            (_, SpecificationAssetClass::FungibleToken) => Interface::FungibleToken,
            _ => Interface::Custom,
        }
    }
}

impl From<Interface> for (SpecificationVersions, SpecificationAssetClass) {
    fn from(interface: Interface) -> (SpecificationVersions, SpecificationAssetClass) {
        match interface {
            Interface::V1NFT => (SpecificationVersions::V1, SpecificationAssetClass::Nft),
            Interface::LEGACY_NFT => (SpecificationVersions::V0, SpecificationAssetClass::Nft),
            Interface::ProgrammableNFT => (
                SpecificationVersions::V1,
                SpecificationAssetClass::ProgrammableNft,
            ),
            Interface::V1PRINT => (SpecificationVersions::V1, SpecificationAssetClass::Print),
            Interface::FungibleAsset => (
                SpecificationVersions::V1,
                SpecificationAssetClass::FungibleAsset,
            ),
            Interface::MplCoreAsset => (
                SpecificationVersions::V1,
                SpecificationAssetClass::MplCoreAsset,
            ),
            Interface::MplCoreCollection => (
                SpecificationVersions::V1,
                SpecificationAssetClass::MplCoreCollection,
            ),
            Interface::FungibleToken => (
                SpecificationVersions::V1,
                SpecificationAssetClass::FungibleToken,
            ),
            _ => (SpecificationVersions::V1, SpecificationAssetClass::Unknown),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Quality {
    #[serde(rename = "$$schema")]
    pub schema: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub enum Context {
    #[serde(rename = "wallet-default")]
    WalletDefault,
    #[serde(rename = "web-desktop")]
    WebDesktop,
    #[serde(rename = "web-mobile")]
    WebMobile,
    #[serde(rename = "app-mobile")]
    AppMobile,
    #[serde(rename = "app-desktop")]
    AppDesktop,
    #[serde(rename = "app")]
    App,
    #[serde(rename = "vr")]
    Vr,
}

pub type Contexts = Vec<Context>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct File {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Contexts>,
}

pub type Files = Vec<File>;

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct MetadataMap(BTreeMap<String, serde_json::Value>);

impl MetadataMap {
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub const fn inner(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.0
    }

    pub fn set_item(&mut self, key: &str, value: serde_json::Value) -> &mut Self {
        self.0.insert(key.to_string(), value);
        self
    }

    pub fn get_item(&self, key: &str) -> Option<&serde_json::Value> {
        self.0.get(key)
    }
}

// TODO sub schema support
pub type Links = HashMap<String, serde_json::Value>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Content {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub json_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Files>,
    pub metadata: MetadataMap,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Links>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Scope {
    #[serde(rename = "full")]
    Full,
    #[serde(rename = "royalty")]
    Royalty,
    #[serde(rename = "metadata")]
    Metadata,
    #[serde(rename = "extension")]
    Extension,
}

impl From<String> for Scope {
    fn from(s: String) -> Self {
        match &*s {
            "royalty" => Scope::Royalty,
            "metadata" => Scope::Metadata,
            "extension" => Scope::Extension,
            _ => Scope::Full,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Authority {
    pub address: String,
    pub scopes: Vec<Scope>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Compression {
    pub eligible: bool,
    pub compressed: bool,
    pub data_hash: String,
    pub creator_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_data_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u8>,
    pub asset_hash: String,
    pub tree: String,
    pub seq: i64,
    pub leaf_id: i64,
}

pub type GroupKey = String;
pub type GroupValue = String;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Group {
    pub group_key: String,
    pub group_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_metadata: Option<MetadataMap>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub enum RoyaltyModel {
    #[serde(rename = "creators")]
    Creators,
    #[serde(rename = "fanout")]
    Fanout,
    #[serde(rename = "single")]
    Single,
}

impl From<String> for RoyaltyModel {
    fn from(s: String) -> Self {
        match &*s {
            "creators" => RoyaltyModel::Creators,
            "fanout" => RoyaltyModel::Fanout,
            "single" => RoyaltyModel::Single,
            _ => RoyaltyModel::Creators,
        }
    }
}

#[cfg(feature = "sql_types")]
impl From<RoyaltyTargetType> for RoyaltyModel {
    fn from(s: RoyaltyTargetType) -> Self {
        match s {
            RoyaltyTargetType::Creators => RoyaltyModel::Creators,
            RoyaltyTargetType::Fanout => RoyaltyModel::Fanout,
            RoyaltyTargetType::Single => RoyaltyModel::Single,
            _ => RoyaltyModel::Creators,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Royalty {
    pub royalty_model: RoyaltyModel,
    pub target: Option<String>,
    pub percent: f64,
    pub basis_points: u32,
    pub primary_sale_happened: bool,
    pub locked: bool,
}

pub type Address = String;
pub type Share = String;
pub type Verified = bool;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Creator {
    pub address: String,
    pub share: i32,
    pub verified: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema, Default)]
pub enum OwnershipModel {
    #[serde(rename = "single")]
    Single,
    #[default]
    #[serde(rename = "token")]
    Token,
}

impl From<String> for OwnershipModel {
    fn from(s: String) -> Self {
        match &*s {
            "single" => OwnershipModel::Single,
            "token" => OwnershipModel::Token,
            _ => OwnershipModel::Single,
        }
    }
}

#[cfg(feature = "sql_types")]
impl From<OwnerType> for OwnershipModel {
    fn from(s: OwnerType) -> Self {
        match s {
            OwnerType::Token => OwnershipModel::Token,
            OwnerType::Single => OwnershipModel::Single,
            _ => OwnershipModel::Single,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct Ownership {
    pub frozen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_transferable: Option<bool>,
    pub delegated: bool,
    pub delegate: Option<String>,
    pub ownership_model: OwnershipModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum UseMethod {
    Burn,
    Multiple,
    Single,
}

impl From<String> for UseMethod {
    fn from(s: String) -> Self {
        match &*s {
            "Burn" => UseMethod::Burn,
            "Single" => UseMethod::Single,
            "Multiple" => UseMethod::Multiple,
            _ => UseMethod::Single,
        }
    }
}

pub type Mutability = bool;

impl From<ChainMutability> for Mutability {
    fn from(s: ChainMutability) -> Self {
        match s {
            ChainMutability::Mutable => true,
            ChainMutability::Immutable => false,
            _ => true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Uses {
    pub use_method: UseMethod,
    pub remaining: u64,
    pub total: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Supply {
    pub print_max_supply: u64,
    pub print_current_supply: u64,
    pub edition_nonce: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MplCoreInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_minted: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_size: Option<i32>,
    pub plugins_json_version: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenInscriptionInfo {
    pub authority: String,
    pub root: String,
    pub inscription_data: String,
    pub content: String,
    pub encoding: String,
    pub order: u64,
    pub size: u32,
    pub validation_hash: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct TokenInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supply: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_program: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mint_authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub associated_token_address: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct Asset {
    pub interface: Interface,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorities: Option<Vec<Authority>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<Compression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grouping: Option<Vec<Group>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty: Option<Royalty>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creators: Option<Vec<Creator>>,
    pub ownership: Ownership,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<Uses>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supply: Option<Supply>,
    pub mutable: bool,
    pub burnt: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mint_extensions: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inscription: Option<TokenInscriptionInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_info: Option<TokenInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_plugins: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mpl_core_info: Option<MplCoreInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_plugins: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_external_plugins: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct TokenAccount {
    pub address: String,
    pub mint: String,
    pub amount: u64,
    pub owner: String,
    pub frozen: bool,
    pub delegate: Option<String>,
    pub delegated_amount: u64,
    pub close_authority: Option<String>,
    pub extensions: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
#[serde(default)]
pub struct UiTokenAmount {
    pub amount: String,
    pub decimals: u8,
    #[serde(rename = "uiAmount")]
    pub ui_amount: Option<f64>,
    #[serde(rename = "uiAmountString")]
    pub ui_amount_string: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
#[serde(default)]
pub struct RpcTokenAccountBalance {
    pub address: String,
    #[serde(flatten)]
    pub amount: UiTokenAmount,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
#[serde(default)]
pub struct RpcTokenSupply {
    pub amount: String,
    pub decimals: u8,
    #[serde(rename = "uiAmount")]
    pub ui_amount: Option<f64>,
    #[serde(rename = "uiAmountString")]
    pub ui_amount_string: String,
}

#[derive(Serialize, Default, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(default)]
pub struct SolanaRpcContext {
    pub slot: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
#[serde(default)]
pub struct SolanaRpcResponseAndContext<T: Default> {
    pub context: SolanaRpcContext,
    pub value: T,
}
