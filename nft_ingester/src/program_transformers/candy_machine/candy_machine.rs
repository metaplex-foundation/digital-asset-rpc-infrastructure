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
use num_traits::FromPrimitive;
use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DatabaseTransaction, DbBackend,
    EntityTrait, JsonValue,
};

pub async fn candy_machine<'c>(
    candy_machine: &CandyMachine,
    acct: &AccountInfo<'c>,
    txn: &'c DatabaseTransaction,
) -> Result<(), IngesterError> {
    let data = candy_machine.data;

    let candy_machine_data = candy_machine_data::ActiveModel {
        uuid: Set(Some(data.uuid)),
        price: Set(Some(data.price)),
        symbol: Set(data.symbol),
        seller_fee_basis_points: Set(data.seller_fee_basis_points),
        max_suppy: Set(data.max_supply),
        is_mutable: Set(data.is_mutable),
        retain_authority: Set(Some(data.retain_authority)),
        go_live_date: Set(data.go_live_date),
        items_available: Set(data.items_available),
        ..Default::default()
    }
    .insert(txn)
    .await?;

    let candy_machine_state = candy_machine::ActiveModel {
        candy_machine_data_id: Set(candy_machine_data.id),
        features: Set(None),
        authority: Set(candy_machine.authority.to_bytes().to_vec()),
        wallet: Set(candy_machine.wallet.to_bytes().to_vec()),
        token_mint: Set(candy_machine.token_mint.to_bytes().to_vec()),
        items_redeemed: Set(candy_machine.items_redeemed),
        ..Default::default()
    };

    // Do not attempt to modify any existing values:
    // `ON CONFLICT ('id') DO NOTHING`.
    let query = candy_machine::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([candy_machine::Column::Id])
                .do_nothing()
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;

    if candy_machine.data.creators.len() > 0 {
        let mut creators = Vec::with_capacity(candy_machine.data.creators.len());
        for c in metadata.creators.iter() {
            creators.push(candy_machine_creators::ActiveModel {
                candy_machine_data_id: Set(candy_machine_data.id),
                creator: Set(c.address.to_bytes().to_vec()),
                share: Set(c.share as i32),
                verified: Set(c.verified),
                ..Default::default()
            });
        }

        // Do not attempt to modify any existing values:
        // `ON CONFLICT ('asset_id') DO NOTHING`.
        let query = candy_machine_creators::Entity::insert_many(creators)
            .on_conflict(
                OnConflict::columns([candy_machine_creators::Column::CandyMachineDataId])
                    .do_nothing()
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        txn.execute(query).await?;
    };

    if let Some(whitelist_mint_setting) = data.whitelist_mint_settings {
        let candy_machine_whitelist_mint_settings =
            candy_machine_whitelist_mint_settings::ActiveModel {
                candy_machine_data_id: Set(candy_machine_data.id),
                mode: Set(whitelist_mint_settings.mode),
                mint: Set(whitelist_mint_settings.mint.to_bytes().to_vec()),
                presale: Set(whitelist_mint_settings.presale),
                discount_price: Set(whitelist_mint_settings.discount_price),
                ..Default::default()
            };

        let query = candy_machine_whitelist_mint_settings::Entity::insert_one(
            candy_machine_whitelist_mint_settings,
        )
        .on_conflict(
            OnConflict::columns([
                candy_machine_whitelist_mint_settings::Column::CandyMachineDataId,
            ])
            .do_nothing()
            .to_owned(),
        )
        .build(DbBackend::Postgres);
        txn.execute(query).await?
    }

    if let Some(gatekeeper) = data.gatekeeper {
        let candy_machine_gatekeeper = candy_machine_gatekeeper::ActiveModel {
            candy_machine_data_id: Set(candy_machine_data.id),
            gatekeeper_network: Set(gatekeeper.gatekeeper_network.to_bytes().to_vec()),
            expire_on_use: Set(gatekeeper.expire_on_use),
            ..Default::default()
        };

        let query = candy_machine_gatekeeper::Entity::insert_one(candy_machine_gatekeeper)
            .on_conflict(
                OnConflict::columns([candy_machine_gatekeeper::Column::CandyMachineDataId])
                    .do_nothing()
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        txn.execute(query).await?;
    }

    if let Some(end_settings) = data.end_settings {
        let candy_machine_end_settings = candy_machine_end_settings::ActiveModel {
            candy_machine_data_id: Set(candy_machine_data.id),
            number: Set(end_settings.number),
            end_setting_type: Set(end_settings.end_setting_type),
            ..Default::default()
        };

        let query = candy_machine_end_settings::Entity::insert_one(candy_machine_end_settings)
            .on_conflict(
                OnConflict::columns([candy_machine_end_settings::Column::CandyMachineDataId])
                    .do_nothing()
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        txn.execute(query).await?;
    }
    // TODO: fix hidden_settings db structure
    // TODO: fix error handling look at collections
    Ok(())
}
