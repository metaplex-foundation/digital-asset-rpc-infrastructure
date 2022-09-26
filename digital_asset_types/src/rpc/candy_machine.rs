#[cfg(feature = "sql_types")]
use crate::dao::sea_orm_active_enums::{EndSettingType, WhitelistMintMode};
use {
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Creator {
    pub address: String,
    pub share: i32,
    pub verified: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ConfigLineSettings {
    pub prefix_name: String,
    pub name_length: u32,
    pub prefix_uri: String,
    pub uri_length: u32,
    pub is_sequential: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HiddenSettings {
    pub name: String,
    pub uri: String,
    pub hash: [u8; 32],
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum EndSettingModel {
    #[serde(rename = "date")]
    Date,
    #[serde(rename = "amount")]
    Amount,
}

impl From<String> for EndSettingModel {
    fn from(s: String) -> Self {
        match &*s {
            "date" => EndSettingModel::Date,
            "amount" => EndSettingModel::Amount,
        }
    }
}

#[cfg(feature = "sql_types")]
impl From<EndSettingType> for EndSettingModel {
    fn from(s: EndSettingType) -> Self {
        match s {
            EndSettingType::Date => EndSettingModel::Date,
            EndSettingType::Amount => EndSettingModel::Amount,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EndSettings {
    pub end_setting_type: EndSettingType,
    pub number: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FreezeInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_thaw: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frozen_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mint_start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_fee: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Gatekeeper {
    pub gatekeeper_network: String,
    pub expire_on_use: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum WhitelistMintModel {
    #[serde(rename = "burn_every_time")]
    BurnEveryTime,
    #[serde(rename = "never_burn")]
    NeverBurn,
}

impl From<String> for WhitelistMintModel {
    fn from(s: String) -> Self {
        match &*s {
            "burn_every_time" => WhitelistMintModel::BurnEveryTime,
            "never_burn" => WhitelistMintModel::NeverBurn,
        }
    }
}

#[cfg(feature = "sql_types")]
impl From<WhitelistMintMode> for WhitelistMintModel {
    fn from(s: WhitelistMintMode) -> Self {
        match s {
            WhitelistMintMode::BurnEveryTime => WhitelistMintModel::BurnEveryTime,
            WhitelistMintMode::NeverBurn => WhitelistMintModel::NeverBurn,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WhitelistMintSettings {
    pub mode: WhitelistMintMode,
    pub mint: String,
    pub presale: bool,
    pub discount_price: Option<u64>,
}

// TODO fill this out
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CandyGuard {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CandyMachineData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<u64>,
    pub symbol: String,
    pub seller_fee_basis_points: u16,
    pub max_supply: u64,
    pub is_mutable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain_authority: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go_live_date: Option<i64>,
    pub items_available: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_line_settings: Option<ConfigLineSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_settings: Option<HiddenSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_settings: Option<EndSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gatekeeper: Option<Gatekeeper>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whitelist_mint_settings: Option<WhitelistMintSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creators: Option<Vec<Creator>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CandyMachine {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_info: Option<FreezeInfo>,
    pub data: CandyMachineData,
    pub authority: String,
    pub wallet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_mint: Option<String>,
    pub items_redeemed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candy_guard: Option<CandyGuard>,
}
