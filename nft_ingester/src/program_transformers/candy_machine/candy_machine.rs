use crate::IngesterError;

use chrono::Utc;
use digital_asset_types::dao::{
    candy_machine, candy_machine_creators, candy_machine_data,
    generated::sea_orm_active_enums::{EndSettingType, WhitelistMintMode},
    prelude::CandyMachine,
};

use blockbuster::programs::candy_machine::state::CandyMachine as CandyMachineState;
use plerkle_serialization::Pubkey as FBPubkey;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DatabaseConnection,
    DatabaseTransaction, DbBackend, DbErr, EntityTrait,
};

pub async fn candy_machine<'c>(
    candy_machine: &CandyMachineState,
    id: FBPubkey,
    txn: &DatabaseTransaction,
    db: &DatabaseConnection,
) -> Result<(), IngesterError> {
    let data = candy_machine.clone().data;

    let token_mint = if let Some(token_mint) = candy_machine.token_mint {
        Some(token_mint.to_bytes().to_vec())
    } else {
        None
    };

    let candy_machine_model: Option<candy_machine::Model> =
        CandyMachine::find_by_id(id.0.to_vec()).one(db).await?;

    let last_minted = if let Some(candy_machine_model) = candy_machine_model {
        if candy_machine_model.items_redeemed < candy_machine.items_redeemed as i64 {
            Some(Utc::now())
        } else {
            candy_machine_model.last_minted
        }
    } else {
        None
    };

    let candy_machine_state = candy_machine::ActiveModel {
        id: Set(id.0.to_vec()),
        authority: Set(candy_machine.authority.to_bytes().to_vec()),
        wallet: Set(Some(candy_machine.wallet.to_bytes().to_vec())),
        token_mint: Set(token_mint),
        items_redeemed: Set(candy_machine.items_redeemed as i64),
        created_at: Set(Some(Utc::now())),
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

    txn.execute(query)
        .await
        .map(|_| ())
        .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;

    let (mode, presale, whitelist_mint, discount_price) =
        if let Some(whitelist) = data.whitelist_mint_settings {
            let mode = match whitelist.mode {
                blockbuster::programs::candy_machine::state::WhitelistMintMode::BurnEveryTime => {
                    WhitelistMintMode::BurnEveryTime
                }
                blockbuster::programs::candy_machine::state::WhitelistMintMode::NeverBurn => {
                    WhitelistMintMode::NeverBurn
                }
            };

            let discount_price = whitelist.discount_price.unwrap() as i64;
            (
                Some(mode),
                Some(whitelist.presale),
                Some(whitelist.mint.to_bytes().to_vec()),
                Some(discount_price),
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
        let end_settings_type = match end_settings.end_setting_type {
            blockbuster::programs::candy_machine::state::EndSettingType::Date => {
                EndSettingType::Date
            }
            blockbuster::programs::candy_machine::state::EndSettingType::Amount => {
                EndSettingType::Amount
            }
        };

        (Some(end_settings_type), Some(end_settings.number as i64))
    } else {
        (None, None)
    };

    let candy_machine_data = candy_machine_data::ActiveModel {
        candy_machine_id: Set(id.0.to_vec()),
        uuid: Set(Some(data.uuid)),
        price: Set(Some(data.price as i64)),
        symbol: Set(data.symbol),
        seller_fee_basis_points: Set(data.seller_fee_basis_points as i16),
        max_supply: Set(data.max_supply as i64),
        is_mutable: Set(data.is_mutable),
        retain_authority: Set(Some(data.retain_authority)),
        go_live_date: Set(data.go_live_date),
        items_available: Set(data.items_available as i64),
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

    txn.execute(query)
        .await
        .map(|_| ())
        .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;

    if candy_machine.data.creators.len() > 0 {
        let mut creators = Vec::with_capacity(candy_machine.data.creators.len());
        for c in candy_machine.data.creators.iter() {
            creators.push(candy_machine_creators::ActiveModel {
                candy_machine_id: Set(id.0.to_vec()),
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

        txn.execute(query)
            .await
            .map(|_| ())
            .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;
    }
    Ok(())
}
