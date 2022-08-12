use num_derive::FromPrimitive;
use solana_sdk::pubkey::Pubkey;

#[cfg(feature = "json_types")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "json_types", derive(Deserialize, Serialize))]
#[derive(PartialEq, Copy, Clone, Debug, FromPrimitive)]
pub enum TokenProgramVersion {
    Original,
    Token2022,
}

#[cfg_attr(feature = "json_types", derive(Deserialize, Serialize))]
#[derive(PartialEq, Copy, Clone, Debug)]
pub struct Creator {
    pub address: Pubkey,
    pub verified: bool,
    // In percentages, NOT basis points ;) Watch out!
    pub share: u8,
}

#[cfg_attr(feature = "json_types", derive(Deserialize, Serialize))]
#[derive(PartialEq, Copy, Clone, Debug, FromPrimitive)]
pub enum TokenStandard {
    NonFungible,        // This is a master edition
    FungibleAsset,      // A token with metadata that can also have attrributes
    Fungible,           // A token with simple metadata
    NonFungibleEdition, // This is a limited edition
}

#[cfg_attr(feature = "json_types", derive(Deserialize, Serialize))]
#[derive(PartialEq, Copy, Clone, Debug, FromPrimitive)]
pub enum UseMethod {
    Burn,
    Multiple,
    Single,
}

#[cfg_attr(feature = "json_types", derive(Deserialize, Serialize))]
#[derive(PartialEq, Copy, Clone, Debug)]
pub struct Uses {
    // 17 bytes + Option byte
    pub use_method: UseMethod, //1
    pub remaining: u64,        //8
    pub total: u64,            //8
}

#[repr(C)]
#[cfg_attr(feature = "json_types", derive(Deserialize, Serialize))]
#[derive(PartialEq, Copy, Clone, Debug)]
pub struct Collection {
    pub verified: bool,
    pub key: Pubkey,
}
