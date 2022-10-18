
pub const FREEZE_FEE: u64 = 0; //100000; // 0.0001 SOL
pub const DEFAULT_UUID: &str = "ABCDEF";
pub const DEFAULT_PRICE: u64 = 1;
// TODO figure out encoding on this for DB
pub const DEFAULT_SYMBOL: &str = "SYMBOL0000";

pub const MAX_CREATOR_LEN: usize = 32 + 1 + 1;
pub const MAX_NAME_LENGTH: usize = 32;

pub const MAX_SYMBOL_LENGTH: usize = 10;

pub const MAX_URI_LENGTH: usize = 200;
pub const MAX_CREATOR_LIMIT: usize = 5;

pub const CONFIG_LINE_SIZE: usize = 4 + MAX_NAME_LENGTH + 4 + MAX_URI_LENGTH;

/// Start index of the config data in the PDA (offset calculated in bytes).
pub const CONFIG_ARRAY_START: usize = 8 +   // key
    32 +                                    // authority
    32 +                                    // wallet
    33 +                                    // token mint
    4 + 6 +                                 // uuid
    8 +                                     // price
    8 +                                     // items available
    9 +                                     // go live
    10 +                                    // end settings
    4 + MAX_SYMBOL_LENGTH +                 // u32 len + symbol
    2 +                                     // seller fee basis points
    4 + MAX_CREATOR_LIMIT*MAX_CREATOR_LEN + // optional + u32 len + actual vec
    8 +                                     // max supply
    1 +                                     // is mutable
    1 +                                     // retain authority
    1 +                                     // option for hidden setting
    4 + MAX_NAME_LENGTH +                   // name length
    4 + MAX_URI_LENGTH +                    // uri length
    32 +                                    // hash
    4 +                                     // max number of lines
    8 +                                     // items redeemed
    1 +                                     // whitelist option
    1 +                                     // whitelist mint mode
    1 +                                     // allow presale
    9 +                                     // discount price
    32 +                                    // mint key for whitelist
    1 + 32 + 1                              // gatekeeper
;
