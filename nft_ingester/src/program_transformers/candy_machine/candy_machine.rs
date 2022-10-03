use crate::{program_transformers::candy_machine::state::CandyMachine, IngesterError};

use chrono::Utc;
use digital_asset_types::dao::generated::{
    candy_machine, candy_machine_creators, candy_machine_data,
};

use plerkle_serialization::AccountInfo;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DatabaseTransaction, DbBackend,
    EntityTrait,
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

    let candy_machine_model: Option<candy_machine::Model> =
        CandyMachine::find_by_id(acct.key().to_bytes().to_vec())
            .one(db)
            .await?;

    let last_minted = if let Some(candy_machine_model) = candy_machine_model {
        if candy_machine_model.items_redeemed < candy_machine.items_redeemed {
            Some(Utc::now())
        } else {
            Some(candy_machine_model.items_redeemed)
        }
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
        created_at: Set(Utc::now()),
        last_minted: Set(last_minted),
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
                    candy_machine::Column::LastMinted,
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
            Some(hidden_settings.hash.to_vec()),
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
        whitelist_mode: Set(mode),
        whitelist_mint: Set(whitelist_mint),
        whitelist_presale: Set(presale),
        whitelist_discount_price: Set(discount_price),
        gatekeeper_network: Set(gatekeeper_network),
        gatekeeper_expire_on_use: Set(expire_on_use),
        end_setting_number: Set(number),
        end_setting_type: Set(end_setting_type),
        hidden_settings_name: Set(name),
        hidden_settings_uri: Set(uri),
        hidden_settings_hash: Set(hash),
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
                    candy_machine_data::Column::WhitelistMode,
                    candy_machine_data::Column::WhitelistMint,
                    candy_machine_data::Column::WhitelistPresale,
                    candy_machine_data::Column::WhitelistDiscountPrice,
                    candy_machine_data::Column::GatekeeperNetwork,
                    candy_machine_data::Column::GatekeeperExpireOnUse,
                    candy_machine_data::Column::EndSettingNumber,
                    candy_machine_data::Column::EndSettingType,
                    candy_machine_data::Column::HiddenSettingsName,
                    candy_machine_data::Column::HiddenSettingsUri,
                    candy_machine_data::Column::HiddenSettingsHash,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    if candy_machine_data.creators.len() > 0 {
        let mut creators = Vec::with_capacity(candy_machine.data.creators.len());
        for c in candy_machine.data.creators.iter() {
            creators.push(candy_machine_creators::ActiveModel {
                candy_machine_id: Set(candy_machine.id.to_bytes().to_vec()),
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
