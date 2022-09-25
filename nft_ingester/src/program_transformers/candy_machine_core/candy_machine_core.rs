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

    let candy_machine_data = candy_machine_data::ActiveModel {
        candy_machine_id: Set(acct.key().to_bytes().to_vec()),
        uuid: Set(None),
        price: Set(None),
        symbol: Set(data.symbol),
        seller_fee_basis_points: Set(data.seller_fee_basis_points),
        max_suppy: Set(data.max_supply),
        is_mutable: Set(data.is_mutable),
        retain_authority: Set(None),
        go_live_date: Set(data.go_live_date),
        items_available: Set(data.items_available),
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

    if candy_machine.data.creators.len() > 0 {
        process_creators_change(candy_machine.data.creators, candy_machine_data_id, txn).await?;
    };

    if let Some(config_line_settings) = data.config_line_settings {
        process_config_line_change(&config_line_settings, candy_machine_data_id, txn).await?;
    }

    if let Some(hidden_settings) = data.hidden_settings {
        process_hidden_settings_change(&hidden_settings, candy_machine_data_id, txn).await?;
    }

    Ok(())
}
