use crate::{
    program_transformers::{
        candy_guard::helpers::process_config_line_change,
        candy_machine::helpers::{process_creators_change, process_hidden_settings_change},
        common::save_changelog_event,
    },
    IngesterError,
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use digital_asset_types::{
    adapter::{TokenStandard, UseMethod, Uses},
    dao::{candy_machine, candy_machine_config_line_settings, candy_machine_data},
    json::ChainDataV1,
};
use mpl_candy_machine_core::CandyMachine;
use num_traits::FromPrimitive;
use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DatabaseTransaction, DbBackend,
    EntityTrait, JsonValue,
};

pub async fn candy_machine_core<'c>(
    candy_machine_core: &CandyMachine,
    acct: &AccountInfo<'c>,
    txn: &'c DatabaseTransaction,
) -> Result<(), IngesterError> {
    let data = candy_machine_core.data;

    let candy_machine_core = candy_machine::ActiveModel {
        id: Set(acct.key().to_bytes().to_vec()),
        features: Set(Some(candy_machine.features)),
        authority: Set(candy_machine.authority.to_bytes().to_vec()),
        wallet: Set(candy_machine.wallet.to_bytes().to_vec()),
        token_mint: Set(candy_machine.token_mint.to_bytes().to_vec()),
        items_redeemed: Set(candy_machine.items_redeemed),
        mint_authority: Set(candy_machine.mint_authority.to_bytes().to_vec()),
        version: Set(3),
        candy_guard_pda: Set(None),
        candy_guard_pda: Set(None),
        collection_mint: Set(None),
        allow_thaw: Set(None),
        frozen_count: Set(None),
        mint_start: Set(None),
        freeze_time: Set(None),
        freeze_fee: Set(None),
    };

    // TODO should consider moving settings back to part of data ?

    let query = candy_machine::Entity::insert(candy_machine_core)
        .on_conflict(
            OnConflict::columns([candy_machine::Column::Id])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    let (name, uri, hash) = if let Some(hidden_settings) = data.hidden_settings {
        (
            Some(hidden_settings.name),
            Some(hidden_settings.uri),
            Some(hidden_settings.hash),
        )
    } else {
        (None, None, None)
    };

    let (prefix_name, name_length, prefix_uri, uri_length) =
        if let Some(config_line_settings) = data.config_line_settings {
            (
                Some(config_line_settings.prefix_name),
                Some(config_line_settings.name_length),
                Some(config_line_settings.prefix_uri),
                Some(config_line_settings.uri_length),
                Some(config_line_settings.is_sequential),
            )
        } else {
            (None, None, None, None, None)
        };

    let candy_machine_data = candy_machine_data::ActiveModel {
        candy_machine_id: Set(acct.key().to_bytes().to_vec()),
        uuid: Set(None),
        price: Set(None),
        symbol: Set(data.symbol),
        seller_fee_basis_points: Set(data.seller_fee_basis_points),
        max_supply: Set(data.max_supply),
        is_mutable: Set(data.is_mutable),
        retain_authority: Set(None),
        go_live_date: Set(data.go_live_date),
        items_available: Set(data.items_available),
        mode: Set(None),
        whitelist_mint: Set(None),
        presale: Set(None),
        discount_price: Set(None),
        gatekeeper_network: Set(None),
        expire_on_use: Set(None),
        prefix_name: Set(prefix_name),
        name_length: Set(name_length),
        prefix_uri: Set(prefix_uri),
        uri_length: Set(uri_length),
        is_sequential: Set(is_sequential),
        number: Set(None),
        end_setting_type: Set(None),
        name: Set(name),
        uri: Set(uri),
        hash: Set(hash),
        ..Default::default()
    };

    let query = candy_machine_data::Entity::insert(candy_machine_data)
        .on_conflict(
            OnConflict::columns([candy_machine_data::Column::Id])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    if candy_machine_data.creators.len() > 0 {
        let mut creators = Vec::with_capacity(candy_machine.data.creators.len());
        for c in metadata.creators.iter() {
            creators.push(candy_machine_creators::ActiveModel {
                candy_machine_id: Set(candy_machine_id),
                creator: Set(c.address.to_bytes().to_vec()),
                share: Set(c.share as i32),
                verified: Set(c.verified),
                ..Default::default()
            });
        }

        let query = candy_machine_creators::Entity::insert_many(creators)
            .on_conflict(
                OnConflict::columns([candy_machine_creators::Column::CandyMachineId])
                    .do_nothing()
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        txn.execute(query).await.map(|_| ()).map_err(Into::into);
    }

    Ok(())
}
