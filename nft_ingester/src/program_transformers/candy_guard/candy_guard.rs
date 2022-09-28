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
    let candy_guard = candy_guard::ActiveModel {
        id: Set(candy_guard.base.to_bytes().to_vec()),
        bump: Set(candy_guard.bump),
        authority: Set(candy_guard.authority.to_bytes().to_vec()),
    };

    // TODO need to get from DB for value cm and update the candy guard pda value
    let query = candy_guard::Entity::insert(candy_guard)
        .on_conflict(
            OnConflict::columns([candy_guard::Column::Id])
                .update_columns([candy_guard::Column::Bump, candy_guard::Column::Authority])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    let (mode, presale, whitelist_mint, discount_price) =
        if let Some(whitelist) = data.whitelist_mint_settings {
            (
                Some(whitelist.mode),
                Some(whitelist.presale),
                Some(whitelist.mint.to_bytes().to_vec()),
                whitelist.discount_price,
            )
        } else {
            (None, None, None, None)
        };

    let (expire_on_use, gatekeeper_network) = if let Some(gatekeeper) = data.gatekeeper {
        (
            Some(gatekeeper.expire_on_use),
            Some(gatekeeper.gatekeeper_network.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    };

    // TODO put all these helpers in sep file
    let (end_setting_type, number) = if let Some(end_settings) = data.end_settings {
        (
            Some(end_settings.end_setting_type),
            Some(end_settings.number),
        )
    } else {
        (None, None)
    };

    let candy_guard_default_set = candy_guard_group::ActiveModel {
        label: Set(None),
        candy_guard_id: Set(candy_guard.base.to_bytes().to_vec()),
        mode: Set(mode),
        whitelist_mint: Set(whitelist_mint),
        presale: Set(presale),
        discount_price: Set(discount_price),
        gatekeeper_network: Set(gatekeeper_network),
        expire_on_use: Set(expire_on_use),
        number: Set(number),
        end_setting_type: Set(end_setting_type),
        merkle_root: todo!(),
        amount: todo!(),
        destination: todo!(),
        signer_key: todo!(),
        limit: todo!(),
        burn: todo!(),
        required_collection: todo!(),
        lamports: todo!(),
        last_instruction: todo!(),
        live_date: todo!(),
        spl_token_amount: todo!(),
        token_mint: todo!(),
        destination_ata: todo!(),
        ..Default::default()
    };

    // TODO finish filling this out ^^
    if let Some(groups) = candy_guard_data.groups {
        if groups.len() > 0 {
            for g in groups.iter() {
                let candy_guard_group = candy_guard_group::ActiveModel {
                    label: Set(Some(g.label)),
                    candy_guard_id: Set(candy_guard.base.to_bytes().to_vec()),
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
            }
        };
    }

    Ok(())
}
