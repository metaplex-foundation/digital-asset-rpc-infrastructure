use crate::{
    program_transformers::{
        candy_machine::helpers::{process_creators_change, process_hidden_settings_change},
        common::save_changelog_event,
    },
    IngesterError,
};
use digital_asset_types::dao::prelude::{
    CandyGuard, CandyGuardGroup, CandyMachine as CandyMachineEntity, CandyMachineData,
};

use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use digital_asset_types::dao::{candy_machine, candy_machine_data};
use mpl_candy_machine_core::CandyMachine;
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
    let collection_mint = if let Some(collection_mint) = candy_machine_core.collection_mint {
        Some(collection_mint.to_bytes().to_vec())
    } else {
        None
    };

    let mint_authority = if let Some(mint_authority) = candy_machine_core.mint_authority {
        Some(mint_authority.to_bytes().to_vec())
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

    let candy_machine_core = candy_machine::ActiveModel {
        id: Set(acct.key().to_bytes().to_vec()),
        features: Set(Some(candy_machine_core.features)),
        authority: Set(candy_machine_core.authority.to_bytes().to_vec()),
        items_redeemed: Set(candy_machine_core.items_redeemed),
        mint_authority: Set(mint_authority),
        collection_mint: Set(collection_mint),
        version: Set(3),
        created_at: Set(Utc::now()),
        last_minted: Set(last_minted),
        ..Default::default()
    };

    let query = candy_machine::Entity::insert(candy_machine_core)
        .on_conflict(
            OnConflict::columns([candy_machine::Column::Id])
                .update_columns([
                    candy_machine::Column::Authority,
                    candy_machine::Column::Features,
                    candy_machine::Column::ItemsRedeemed,
                    candy_machine::Column::MintAuthority,
                    candy_machine::Column::CollectionMint,
                    candy_machine::Column::Version,
                    candy_machine::Column::LastMinted,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    let (name, uri, hash) = if let Some(hidden_settings) = data.hidden_settings {
        (
            Some(hidden_settings.name),
            Some(hidden_settings.uri),
            Some(hidden_settings.hash.to_vec()),
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
        symbol: Set(data.symbol),
        seller_fee_basis_points: Set(data.seller_fee_basis_points),
        max_supply: Set(data.max_supply),
        is_mutable: Set(data.is_mutable),
        go_live_date: Set(data.go_live_date),
        items_available: Set(data.items_available),
        config_line_settings_prefix_name: Set(prefix_name),
        config_line_settings_name_length: Set(name_length),
        config_line_settings_prefix_uri: Set(prefix_uri),
        config_line_settings_uri_length: Set(uri_length),
        config_line_settings_is_sequential: Set(is_sequential),
        hidden_settings_name: Set(name),
        hidden_settings_uri: Set(uri),
        hidden_settings_hash: Set(hash),
        ..Default::default()
    };

    let query = candy_machine_data::Entity::insert(candy_machine_data)
        .on_conflict(
            OnConflict::columns([candy_machine_data::Column::CandyMachineId])
                .update_columns([
                    candy_machine_data::Column::Symbol,
                    candy_machine_data::Column::SellerFeeBasisPoints,
                    candy_machine_data::Column::MaxSupply,
                    candy_machine_data::Column::IsMutable,
                    candy_machine_data::Column::GoLiveDate,
                    candy_machine_data::Column::ItemsAvailable,
                    candy_machine_data::Column::HiddenSettingsName,
                    candy_machine_data::Column::HiddenSettingsUri,
                    candy_machine_data::Column::HiddenSettingsHash,
                    candy_machine_data::Column::ConfigLineSettingsPrefixName,
                    candy_machine_data::Column::ConfigLineSettingsNameLength,
                    candy_machine_data::Column::ConfigLineSettingsPrefixUri,
                    candy_machine_data::Column::ConfigLineSettingsUriLength,
                    candy_machine_data::Column::ConfigLineSettingsIsSequential,
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
