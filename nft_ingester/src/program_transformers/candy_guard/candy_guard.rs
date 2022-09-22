use crate::{
    program_transformers::{
        candy_machine::{helpers::process_whitelist_change, state::CandyMachine},
        common::save_changelog_event,
    },
    IngesterError,
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use candy_machine::state::CandyMachine;
use digital_asset_types::{
    adapter::{TokenStandard, UseMethod, Uses},
    dao::candy_machine_collections,
    json::ChainDataV1,
};
use mpl_candy_guard::state::{CandyGuard, CandyGuardData};
use num_traits::FromPrimitive;
use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DatabaseTransaction, DbBackend,
    EntityTrait, JsonValue,
};
use solana_sdk::lamports;

use super::helpers::{
    process_allow_list_change, process_bot_tax_change, process_guard_set_change,
    process_nft_payment_change, process_third_party_signer_change,
};

pub async fn candy_guard<'c>(
    candy_guard: &CandyGuard,
    candy_guard_data: &CandyGuardData,
    acct: &AccountInfo<'c>,
    txn: &'c DatabaseTransaction,
) -> Result<(), IngesterError> {
    let default_guard_set = candy_guard_data.default;
    //TODO: what to do with CandyGuard here?

    // TODO: find some kind of way to get candy id in here
    process_guard_set_change(&default_guard_set, txn);

    // TODO: should these be inserted and/or updated all in one db trx

    if let Some(groups) = candy_guard_data.groups {
        if groups.len() > 0 {
            for g in groups.iter() {
                let guard_set = g.guards;
                // TODO: add label to DB
                process_guard_set_change(&guard_set, txn);
            }
        };
    }

    Ok(())
}
