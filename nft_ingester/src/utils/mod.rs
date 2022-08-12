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
    Pubkey::new(pubkey.key().unwrap())
}

pub fn string_from_fb_table(
    keys: &Vector<ForwardsUOffset<transaction_info::Pubkey>>,
    index: usize,
) -> String {
    pubkey_from_fb_table(keys, index).to_string()
}

pub fn bytes_from_fb_table<'a>(
    keys: &Vector<
        'a,
        ForwardsUOffset<
            plerkle_serialization::transaction_info_generated::transaction_info::Pubkey<'a>,
        >,
    >,
    index: usize,
) -> Vec<u8> {
    let pubkey = keys.get(index);
    pubkey.key().unwrap().to_vec()
}
