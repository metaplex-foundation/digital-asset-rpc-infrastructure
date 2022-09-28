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
    rpc::HiddenSettings,
};
use mpl_candy_guard::guards::{Gatekeeper, Whitelist};
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

    let token_mint = if let Some(token_mint) = candy_machine.token_mint {
        Some(token_mint.to_bytes().to_vec())
    } else {
        None
    };

    let wallet = if let Some(wallet) = candy_machine.wallet {
        Some(wallet.to_bytes().to_vec())
    } else {
        None
    };

    let candy_machine_state = candy_machine::ActiveModel {
        id: Set(acct.key().to_bytes().to_vec()),
        authority: Set(candy_machine.authority.to_bytes().to_vec()),
        wallet: Set(wallet),
        token_mint: Set(token_mint),
        items_redeemed: Set(candy_machine.items_redeemed),
        version: Set(2),
        ..Default::default()
    };

    let query = candy_machine::Entity::insert(candy_machine_state)
        .on_conflict(
            OnConflict::columns([candy_machine::Column::Id])
                .update_columns([
                    candy_machine::Column::Authority,
                    candy_machine::Column::Wallet,
                    candy_machine::Column::TokenMint,
                    candy_machine::Column::ItemsRedeemed,
                ])
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

    let (name, uri, hash) = if let Some(hidden_settings) = data.hidden_settings {
        (
            Some(hidden_settings.name),
            Some(hidden_settings.uri),
            Some(hidden_settings.hash),
        )
    } else {
        (None, None, None)
    };

    let (expire_on_use, gatekeeper_network) = if let Some(gatekeeper) = data.gatekeeper {
        (
            Some(gatekeeper.expire_on_use),
            Some(gatekeeper.gatekeeper_network.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    };

    let (end_setting_type, number) = if let Some(end_settings) = data.end_settings {
        (
            Some(end_settings.end_setting_type),
            Some(end_settings.number),
        )
    } else {
        (None, None)
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
        mode: Set(mode),
        whitelist_mint: Set(whitelist_mint),
        presale: Set(presale),
        discount_price: Set(discount_price),
        gatekeeper_network: Set(gatekeeper_network),
        expire_on_use: Set(expire_on_use),
        number: Set(number),
        end_setting_type: Set(end_setting_type),
        name: Set(name),
        uri: Set(uri),
        hash: Set(hash),
        ..Default::default()
    };

    let query = candy_machine_data::Entity::insert(candy_machine_data)
        .on_conflict(
            OnConflict::columns([candy_machine_data::Column::CandyMachineId])
                .update_columns([
                    candy_machine_data::Column::Uuid,
                    candy_machine_data::Column::Price,
                    candy_machine_data::Column::Symbol,
                    candy_machine_data::Column::SellerFeeBasisPoints,
                    candy_machine_data::Column::MaxSupply,
                    candy_machine_data::Column::IsMutable,
                    candy_machine_data::Column::RetainAuthority,
                    candy_machine_data::Column::GoLiveDate,
                    candy_machine_data::Column::ItemsAvailable,
                    candy_machine_data::Column::Mode,
                    candy_machine_data::Column::WhitelistMint,
                    candy_machine_data::Column::Presale,
                    candy_machine_data::Column::DiscountPrice,
                    candy_machine_data::Column::GatekeeperNetwork,
                    candy_machine_data::Column::ExpireOnUse,
                    candy_machine_data::Column::Number,
                    candy_machine_data::Column::EndSettingType,
                    candy_machine_data::Column::Name,
                    candy_machine_data::Column::Uri,
                    candy_machine_data::Column::Hash,
                ])
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
                    .update_columns([
                        candy_machine_creators::Column::Creator,
                        candy_machine_creators::Column::Share,
                        candy_machine_creators::Column::Verified,
                    ])
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        txn.execute(query).await.map(|_| ()).map_err(Into::into);
    }
    Ok(())
}
