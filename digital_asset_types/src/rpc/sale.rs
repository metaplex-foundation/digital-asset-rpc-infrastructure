use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Offer {
    pub from: String,
    pub amount: u64,
    pub price: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct AssetSale {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listing_id: Option<String>,
    pub asset_id: String,
    pub amount: u64,
    pub price: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highest_offers: Option<Offer>,
}
