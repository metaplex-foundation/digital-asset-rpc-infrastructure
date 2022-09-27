use crate::dao::prelude::{CandyGuard, CandyGuardGroup, CandyMachine, CandyMachineData};
use crate::dao::{
    candy_guard, candy_guard_group, candy_machine, candy_machine_creators, candy_machine_data,
};
use crate::rpc::{
    CandyGuard as RpcCandyGuard, CandyMachine as RpcCandyMachine,
    CandyMachineData as RpcCandyMachineData, ConfigLineSettings, Creator, EndSettings, FreezeInfo,
    Gatekeeper, HiddenSettings, WhitelistMintSettings, CandyGuardData,
};
use jsonpath_lib::JsonPathError;
use mime_guess::Mime;
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use url::Url;

pub fn to_creators(creators: Vec<candy_machine_creators::Model>) -> Vec<Creator> {
    creators
        .iter()
        .map(|a| Creator {
            address: bs58::encode(&a.creator).into_string(),
            share: a.share,
            verified: a.verified,
        })
        .collect()
}

pub fn get_freeze_info(
    allow_thaw: Option<bool>,
    frozen_count: Option<u64>,
    freeze_time: Option<i64>,
    freeze_fee: Option<u64>,
    mint_start: Option<i64>,
) -> Option<FreezeInfo> {
    if allow_thaw.is_some()
        && frozen_count.is_some()
        && freeze_time.is_some()
        && freeze_fee.is_some()
    {
        // TODO extract these value from options minus mint_start
        Some(FreezeInfo {
            allow_thaw,
            frozen_count,
            mint_start,
            freeze_time,
            freeze_fee,
        })
    } else {
        None
    }
}

pub fn get_config_line_settings(
    config_line_settings: &Option<ConfigLineSettings>,
) -> Option<ConfigLineSettings> {
    if config_line_settings.is_some() {
        config_line_settings
    } else {
        None
    }
}

pub fn get_hidden_settings(hidden_settings: &Option<HiddenSettings>) -> Option<HiddenSettings> {
    if hidden_settings.is_some() {
        hidden_settings
    } else {
        None
    }
}

pub fn get_end_settings(end_settings: &Option<EndSettings>) -> Option<HiddenSettings> {
    if end_settings.is_some() {
        end_settings
    } else {
        None
    }
}

pub fn get_gatekeeper(gatekeeper: &Option<Gatekeeper>) -> Option<Gatekeeper> {
    if gatekeeper.is_some() {
        gatekeeper
    } else {
        None
    }
}

pub fn get_whitelist_mint_settings(
    whitelist_mint_settings: &Option<WhitelistMintSettings>,
) -> Option<WhitelistMintSettings> {
    if whitelist_mint_settings.is_some() {
        whitelist_mint_settings
    } else {
        None
    }
}

pub async fn get_candy_machine(
    db: &DatabaseConnection,
    candy_machine_id: Vec<u8>,
) -> Result<RpcCandyMachine, DbErr> {
    let (candy_machine, candy_machine_data): (candy_machine::Model, candy_machine_data::Model) =
        CandyMachine::find_by_id(candy_machine_id)
            .find_also_related(CandyMachineData)
            .one(db)
            .await
            .and_then(|o| match o {
                Some((a, Some(d))) => Ok((a, d)),
                _ => Err(DbErr::RecordNotFound("Candy Machine Not Found".to_string())),
            })?;

    let creators: Vec<candy_machine_creators::Model> = candy_machine_creators::Entity::find()
        .filter(candy_machine_creators::Column::CandyMachineId.eq(candy_machine.id.clone()))
        .all(db)
        .await?;

    let candy_guard = if let Some(candy_guard_pda) = candy_machine.candy_guard_pda {
        let (candy_guard, candy_guard_group): (candy_guard::Model, Vec<candy_guard_group::Model>) =
            CandyGuard::find_by_id(candy_guard_pda)
                .find_also_related(CandyGuardGroup)
                .all(db)
                .await
                .and_then(|o| match o {
                    Some((a, Some(d))) => Ok((a, d)),
                    _ => Err(DbErr::RecordNotFound("Candy Guard Not Found".to_string())),
                })?;


         for g in groups.iter(){

        }   
        RpcCandyGuard {
            id: candy_guard.id,
            bump: candy_guard.bump,
            authority: candy_guard.authority,
            candy_guard_data: CandyGuardData{ default: todo!(), groups: todo!() },
        }
    } else {
        None
    };

    let rpc_creators = to_creators(creators);

    let freeze_info = get_freeze_info(
        candy_machine.allow_thaw,
        candy_machine.frozen_count,
        candy_machine.freeze_time,
        candy_machine.freeze_fee,
        candy_machine.mint_start,
    );

    let data_config_line_settings =
        get_config_line_settings(candy_machine_data.config_line_settings);
    let data_hidden_settings = get_hidden_settings(candy_machine_data.hidden_settings);
    let data_end_settings = get_end_settings(candy_machine_data.end_settings);
    let data_gatekeeper = get_gatekeeper(candy_machine_data.gatekeeper);
    let data_whitelist_mint_settings =
        get_whitelist_mint_settings(candy_machine_data.whitelist_mint_settings);

    Ok(RpcCandyMachine {
        id: candy_machine.id,
        collection: candy_machine.collection_mint,
        freeze_info,
        data: RpcCandyMachineData {
            uuid: candy_machine_data.uuid,
            price: candy_machine_data.price,
            symbol: candy_machine_data.symbol,
            seller_fee_basis_points: candy_machine_data.seller_fee_basis_points,
            max_supply: candy_machine_data.max_supply,
            is_mutable: candy_machine_data.is_mutable,
            retain_authority: candy_machine_data.retain_authority,
            go_live_date: candy_machine_data.go_live_date,
            items_available: candy_machine_data.items_available,
            config_line_settings: data_config_line_settings,
            hidden_settings: data_hidden_settings,
            end_settings: data_end_settings,
            gatekeeper: data_gatekeeper,
            whitelist_mint_settings: data_whitelist_mint_settings,
            creators: Some(rpc_creators),
        },
        authority: candy_machine.authority,
        wallet: candy_machine.wallet,
        token_mint: candy_machine.token_mint,
        items_redeemed: candy_machine.items_redeemed,
        candy_guard,
    })
}
