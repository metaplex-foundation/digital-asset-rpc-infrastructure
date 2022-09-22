use blockbuster::programs::candy_machine;
use digital_asset_types::dao::candy_machine_whitelist_mint_settings;
use mpl_candy_guard::guards::{EndSettings, Gatekeeper};
use sea_orm::{entity::*, query::*, sea_query::OnConflict, DatabaseTransaction, DbBackend};

use crate::error::IngesterError;

use super::state::WhitelistMintSettings;

pub async fn process_whitelist_change(
    whitelist_mint_settings: &WhitelistMintSettings,
    candy_machine_data_id: i64,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_machine_whitelist_mint_settings =
        candy_machine_whitelist_mint_settings::ActiveModel {
            candy_machine_data_id: Set(candy_machine_data_id),
            mode: Set(whitelist_mint_settings.mode),
            mint: Set(whitelist_mint_settings.mint),
            presale: Set(whitelist_mint_settings.presale),
            discount_price: Set(whitelist_mint_settings.discount_price),
            ..Default::default()
        };

    let query = candy_machine_whitelist_mint_settings::Entity::insert_one(
        candy_machine_whitelist_mint_settings,
    )
    .on_conflict(
        OnConflict::columns([candy_machine_whitelist_mint_settings::Column::CandyMachineDataId])
            .do_nothing()
            .to_owned(),
    )
    .build(DbBackend::Postgres);
    txn.execute(query).await?;

    Ok(())
}

pub async fn process_gatekeeper_change(
    gatekeeper: &Gatekeeper,
    candy_machine_data_id: i64,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
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

    Ok(())
}

pub async fn process_end_settings_change(
    end_settings: &EndSettings,
    candy_machine_data_id: i64,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
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

    Ok(())
}
