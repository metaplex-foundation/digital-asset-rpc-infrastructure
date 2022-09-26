use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum AssetSorting {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "updated")]
    Updated,
    #[serde(rename = "recent_action")]
    RecentAction,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum OfferSorting {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "updated")]
    Updated,
    #[serde(rename = "price")]
    Price,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum ListingSorting {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "updated")]
    Updated,
    #[serde(rename = "price")]
    Price,
    #[serde(rename = "number_of_offers")]
    NumberOfOffers,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum CandyMachineSorting {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "updated")]
    Updated,
    #[serde(rename = "last_mint")]
    LastMint,
}
