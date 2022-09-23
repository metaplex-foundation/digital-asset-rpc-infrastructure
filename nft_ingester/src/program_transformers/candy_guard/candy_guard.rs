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
    dao::{candy_guard, candy_guard_group, candy_machine_collections},
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
    // TODO is this security vulnerability ???? does this need to be stored ??
    let candy_guard = candy_guard::ActiveModel {
        candy_machine_id: Set(acct.key().to_bytes().to_vec()),
        base: Set(candy_guard.base.to_bytes().to_vec()),
        bump: Set(candy_guard.bump),
        authority: Set(candy_guard.authority.to_bytes().to_vec()),
        ..Default::default()
    };

    let query = candy_guard::Entity::insert_one(candy_guard)
        .on_conflict(
            OnConflict::columns([candy_guard::Column::CandyMachineId])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    let candy_guard_group = candy_guard_group::ActiveModel {
        label: Set(None),
        candy_machine_id: Set(acct.key().to_bytes().to_vec()),
        ..Default::default()
    };
    let default_guard_set = candy_guard_data.default;

    process_guard_set_change(&default_guard_set, acct.key().to_bytes().to_vec(), txn);

    // TODO should these be inserted and/or updated all in one db trx

    if let Some(groups) = candy_guard_data.groups {
        if groups.len() > 0 {
            for g in groups.iter() {
                let candy_guard_group = candy_guard_group::ActiveModel {
                    label: Set(Some(g.label)),
                    candy_machine_id: Set(acct.key().to_bytes().to_vec()),
                    ..Default::default()
                };

                let query = candy_guard_group::Entity::insert_one(candy_guard_group)
                    .on_conflict(
                        OnConflict::columns([candy_guard_group::Column::CandyMachineId])
                            .do_nothing()
                            .to_owned(),
                    )
                    .build(DbBackend::Postgres);
                txn.execute(query).await.map(|_| ()).map_err(Into::into);

                let guard_set = g.guards;
                process_guard_set_change(&guard_set, acct.key().to_bytes().to_vec(), txn);
            }
        };
    }

    Ok(())
}
