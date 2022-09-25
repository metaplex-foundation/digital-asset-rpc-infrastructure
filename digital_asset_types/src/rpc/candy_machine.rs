#[cfg(feature = "sql_types")]
use crate::dao::sea_orm_active_enums::{EndSettingType, WhitelistMintMode};
use {
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CandyMachineData {
    pub uuid: Option<String>,
    pub price: Option<u64>,
    pub symbol: String,
    pub seller_fee_basis_points: u16,
    pub max_suppy: u64,
    pub is_mutable: bool,
    pub retain_authority: Option<bool>,
    pub go_live_date: Option<i64>,
    pub items_available: u64,
}

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
pub struct CandyMachine {
    pub id: String,
    pub creators: Option<Vec<Creator>>,
    pub candy_machine_data: CandyMachineData,
    pub config_line_settings: Option<ConfigLineSettings>,
}
