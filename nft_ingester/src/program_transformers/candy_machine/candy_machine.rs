use crate::{
    program_transformers::{candy_machine::state::CandyMachine, common::save_changelog_event},
    IngesterError,
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use candy_machine::state::CandyMachine;
use digital_asset_types::{
    adapter::{TokenStandard, UseMethod, Uses},
    dao::{
        candy_machine, candy_machine_data, candy_machine_end_settings, candy_machine_gatekeeper,
        candy_machine_whitelist_mint_settings,
        sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
    },
    json::ChainDataV1,
};
use mpl_candy_guard::guards::Whitelist;
use num_traits::FromPrimitive;
use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DatabaseTransaction, DbBackend,
    EntityTrait, JsonValue,
};

use super::helpers::{process_candy_machine_change, process_creators_change};

pub async fn candy_machine<'c>(
    candy_machine: &CandyMachine,
    acct: &AccountInfo<'c>,
    txn: &'c DatabaseTransaction,
) -> Result<(), IngesterError> {
    let data = candy_machine.data;

    //TODO should fetch first here ? then update or insert
    let candy_machine_state = candy_machine::ActiveModel {
        id: Set(acct.key().to_bytes().to_vec()),
        features: Set(None),
        authority: Set(candy_machine.authority.to_bytes().to_vec()),
        wallet: Set(candy_machine.wallet.to_bytes().to_vec()),
        token_mint: Set(candy_machine.token_mint.to_bytes().to_vec()),
        items_redeemed: Set(candy_machine.items_redeemed),
        mint_authority: Set(None),
        version: Set(2),
        candy_guard_pda: Set(None),
        collection_mint: Set(None),
        allow_thaw: Set(None),
        frozen_count: Set(None),
        mint_start: Set(None),
        freeze_time: Set(None),
        freeze_fee: Set(None),
    };

    let query = candy_machine::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([candy_machine::Column::Id])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    let (mode, presale, whitelist_mint, discount_price) =
        if let Some(whitelist) = data.whitelist_mint_settings {
            (
                Some(whitelist.mode),
                Some(whitelist.presale),
                Some(whitelist.mint),
                whitelist.discount_price,
            )
        } else {
            (None, None, None, None)
        };

    let candy_machine_data = candy_machine_data::ActiveModel {
        candy_machine_id: Set(acct.key().to_bytes().to_vec()),
        uuid: Set(Some(data.uuid)),
        price: Set(Some(data.price)),
        symbol: Set(data.symbol),
        seller_fee_basis_points: Set(data.seller_fee_basis_points),
        max_supply: Set(data.max_supply),
        is_mutable: Set(data.is_mutable),
        retain_authority: Set(Some(data.retain_authority)),
        go_live_date: Set(data.go_live_date),
        items_available: Set(data.items_available),
        mode: Set(whitelist_mint_settings.mode),
        whitelist_mint: Set(whitelist_mint_settings.mint.to_bytes().to_vec()),
        presale: Set(whitelist_mint_settings.presale),
        discount_price: Set(whitelist_mint_settings.discount_price),
        mint_start: todo!(),
        gatekeeper_network: todo!(),
        expire_on_use: todo!(),
        prefix_name: todo!(),
        name_length: todo!(),
        prefix_uri: todo!(),
        uri_length: todo!(),
        is_sequential: todo!(),
        number: todo!(),
        end_setting_type: todo!(),
    };

    let query = candy_machine_data::Entity::insert(candy_machine_data)
        .on_conflict(
            OnConflict::columns([candy_machine_data::Column::Id])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    process_candy_machine_change(&data, candy_machine_state.id, txn).await?;

    Ok(())
}
