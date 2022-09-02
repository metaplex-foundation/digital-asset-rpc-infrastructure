mod instructions;
mod logs;
mod storage;

pub use instructions::*;
pub use logs::*;
pub use storage::*;

use {
    flatbuffers::{ForwardsUOffset, Vector},
    plerkle_serialization::transaction_info_generated::transaction_info,
    solana_sdk::pubkey::Pubkey,
};

pub fn un_jank_message(hex_str: &String) -> String {
    String::from_utf8(hex::decode(hex_str).unwrap()).unwrap()
}

pub fn pubkey_from_fb_table(
    keys: &Vector<ForwardsUOffset<transaction_info::Pubkey>>,
    index: usize,
) -> Pubkey {
    let pubkey = keys.get(index);
    Pubkey::new(pubkey.0.as_slice())
}

pub fn string_from_fb_table(
    keys: &Vector<ForwardsUOffset<transaction_info::Pubkey>>,
    index: usize,
) -> String {
    pubkey_from_fb_table(keys, index).to_string()
}
