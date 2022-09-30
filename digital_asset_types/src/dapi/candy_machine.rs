use crate::dao::prelude::{CandyGuard, CandyGuardGroup, CandyMachine, CandyMachineData};
use crate::dao::sea_orm_active_enums::WhitelistMintMode;
use crate::dao::{
    candy_guard, candy_guard_group, candy_machine, candy_machine_creators, candy_machine_data,
};
use crate::rpc::{
    AllowList, CandyGuard as RpcCandyGuard, CandyGuardData, CandyMachine as RpcCandyMachine,
    CandyMachineData as RpcCandyMachineData, ConfigLineSettings, Creator, EndSettings, FreezeInfo,
    Gatekeeper, GuardSet, HiddenSettings, Lamports, NftPayment, SplToken, ThirdPartySigner,
    WhitelistMintSettings,
};

use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};

use super::candy_machine_helpers::{get_config_line_settings, get_end_settings};

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

pub fn transform_optional_pubkeys(key: Vec<u8>) -> Option<String> {
    if let Some(pubkey) = key {
        bs58::encode(&pubkey).to_string()
    } else {
        None
    }
}

pub fn get_freeze_info(
    allow_thaw: Option<bool>,
    frozen_count: Option<u64>,
    freeze_time: Option<i64>,
    freeze_fee: Option<u64>,
    mint_start: Option<i64>,
) -> Option<FreezeInfo> {
    if let (Some(allow_thaw), Some(frozen_count), Some(freeze_time), Some(freeze_fee)) =
        (allow_thaw, frozen_count, freeze_time, freeze_fee)
    {
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

pub fn get_hidden_settings(hidden_settings: &Option<HiddenSettings>) -> Option<HiddenSettings> {
    if hidden_settings.is_some() {
        // TODO what to do with hash here ? turn into string? or return as [u8]
        hidden_settings
    } else {
        None
    }
}

pub fn get_gatekeeper(
    gatekeeper_network: &Option<Vec<u8>>,
    gatekeeper_expire_on_use: &Option<bool>,
) -> Option<Gatekeeper> {
    if let (Some(gatekeeper_network), Some(expire_on_use)) =
        (gatekeeper_network, gatekeeper_expire_on_use)
    {
        Some(Gatekeeper {
            gatekeeper_network: bs58::encode(gatekeeper_network).into_string(),
            expire_on_use,
        })
    } else {
        None
    }
}

// TODO fix all these helper methods
pub fn get_whitelist_settings(
    whitelist_mode: Option<WhitelistMintMode>,
    whitelist_mint: Option<Vec<u8>>,
    whitelist_presale: Option<bool>,
    whitelist_discount_price: Option<u64>,
) -> Option<WhitelistMintSettings> {
    if let (Some(whitelist_mode), Some(whitelist_mint), Some(whitelist_presale)) =
        (whitelist_mode, whitelist_mint, whitelist_presale)
    {
        Some(WhitelistMintSettings {
            mode: whitelist_mode,
            mint: bs58::encode(whitelist_mint).into_string(),
            presale: whitelist_presale,
            discount_price: whitelist_discount_price,
        })
    } else {
        None
    }
}

// TODO move all these ^ function to one big match function
pub fn get_lamports(lamports: &Option<Lamports>) -> Option<Lamports> {
    if let Some(lamports) = lamports {
        Some(Lamports {
            amount: lamports.amount,
            destination: bs58::encode(lamports.destination).into_string(),
        })
    } else {
        None
    }
}

pub fn get_allow_list(allow_list: &Option<AllowList>) -> Option<AllowList> {
    if allow_list.is_some() {
        // TODO what to do with merkle root here ? turn into string? or return as [u8]
        allow_list
    } else {
        None
    }
}

pub fn get_nft_payment(
    nft_payment_burn: &Option<bool>,
    nft_payment_required_collection: &Option<Vec<u8>>,
) -> Option<NftPayment> {
    if let (Some(nft_payment_burn), Some(nft_payment_required_collection)) =
        (nft_payment_burn, nft_payment_required_collection)
    {
        Some(NftPayment {
            burn: nft_payment_burn,
            required_collection: bs58::encode(nft_payment_required_collection).into_string(),
        })
    } else {
        None
    }
}

pub fn get_third_party_signer(signer: &Option<ThirdPartySigner>) -> Option<ThirdPartySigner> {
    if let Some(signer) = signer {
        Some(ThirdPartySigner {
            signer_key: bs58::encode(signer.signer_key).to_string(),
        })
    } else {
        None
    }
}

pub fn get_spl_token(
    spl_token_amount: &Option<u64>,
    spl_token_mint: &Option<Vec<u8>>,
    spl_token_destination_ata: &Option<Vec<u8>>,
) -> Option<SplToken> {
    if let (Some(spl_token_amount), Some(spl_token_mint), Some(spl_token_destination_ata)) =
        (spl_token_amount, spl_token_mint, spl_token_destination_ata)
    {
        Some(SplToken {
            amount: spl_token_amount,
            token_mint: bs58::encode(spl_token_mint).to_string(),
            destination_ata: bs58::encode(spl_token_destination_ata).to_string(),
        })
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

        // to make db simpler, all guard sets are added to db as type 'candy_guard_group',
        // since there is always a default but just no label
        // that is the differentiating factor between the two when storing in table
        let default_set = candy_guard_group
            .into_iter()
            .find(|&group| group.label.is_none())
            .map(|&group| {
                let gatekeeper =
                    get_gatekeeper(group.gatekeeper_network, group.gatekeeper_expire_on_use);
                let lamports = get_lamports(group.lamports);
                let spl_token = get_spl_token(
                    group.spl_token_amount,
                    group.spl_token_mint,
                    group.spl_token_destination_ata,
                );
                let third_party_signer = get_third_party_signer(group.third_party_signer);
                let allow_list = get_allow_list(group.allow_list);
                let nft_payment = get_nft_payment(
                    group.nft_payment_burn,
                    group.nft_payment_required_collection,
                );
                let whitelist_settings = get_whitelist_settings(
                    group.whitelist_mode,
                    group.whitelist_mint,
                    group.whitelist_presale,
                    group.whitelist_discount_price,
                );

                GuardSet {
                    bot_tax: group.bot_tax,
                    lamports,
                    spl_token,
                    live_date: group.live_date,
                    third_party_signer,
                    whitelist: whitelist_settings,
                    gatekeeper,
                    end_settings: group.end_settings,
                    allow_list,
                    mint_limit: group.mint_limit,
                    nft_payment,
                }
            })
            .unwrap();

        let groups = Vec::new();
        for group in candy_guard_group
            .iter()
            .filter(|group| group.label.is_some())
        {
            let gatekeeper =
                get_gatekeeper(&group.gatekeeper_network, &group.gatekeeper_expire_on_use);
            let lamports = get_lamports(group.lamports);
            let spl_token = get_spl_token(
                &group.spl_token_amount,
                &group.spl_token_mint,
                &group.spl_token_destination_ata,
            );
            let third_party_signer = get_third_party_signer(group.third_party_signer);
            let allow_list = get_allow_list(group.allow_list);
            let nft_payment = get_nft_payment(
                &group.nft_payment_burn,
                &group.nft_payment_required_collection,
            );
            let whitelist_settings = get_whitelist_settings(
                group.whitelist_mode,
                group.whitelist_mint,
                group.whitelist_presale,
                group.whitelist_discount_price,
            );

            groups.push(GuardSet {
                bot_tax: group.bot_tax,
                lamports,
                spl_token,
                live_date: group.live_date,
                third_party_signer,
                whitelist: whitelist_settings,
                gatekeeper,
                end_settings: group.end_settings,
                allow_list,
                mint_limit: group.mint_limit,
                nft_payment,
            })
            // TODO fix all of these in white ^^ and in related files by size and by creator and live date figure that out
        }

        let is_groups = if groups.len() > 0 { Some(groups) } else { None };

        Some(RpcCandyGuard {
            id: candy_guard.id,
            bump: candy_guard.bump,
            authority: candy_guard.authority,
            candy_guard_data: CandyGuardData {
                default: default_set,
                groups: is_groups,
            },
        })
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

    let data_hidden_settings = get_hidden_settings(candy_machine_data.hidden_settings);
    let data_gatekeeper = get_gatekeeper(
        &candy_machine_data.gatekeeper_network,
        &candy_machine_data.gatekeeper_expire_on_use,
    );
    let data_whitelist_mint_settings = get_whitelist_settings(
        candy_machine_data.whitelist_mode,
        candy_machine_data.whitelist_mint,
        candy_machine_data.whitelist_presale,
        candy_machine_data.whitelist_discount_price,
    );

    let data_end_settings = get_end_settings(
        candy_machine_data.end_setting_number,
        candy_machine_data.end_setting_type,
    );

    let data_config_line_settings = get_config_line_settings(
        candy_machine_data.config_line_settings_is_sequential,
        candy_machine_data.config_line_settings_name_length,
        candy_machine_data.config_line_settings_prefix_name,
        candy_machine_data.config_line_settings_prefix_uri,
        candy_machine_data.config_line_settings_uri_length,
    );

    Ok(RpcCandyMachine {
        id: bs58::encode(candy_machine.id).into_string(),
        collection: transform_optional_pubkeys(candy_machine.collection_mint),
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
        authority: bs58::encode(candy_machine.authority).into_string(),
        wallet: transform_optional_pubkeys(candy_machine.wallet),
        token_mint: transform_optional_pubkeys(candy_machine.token_mint),
        items_redeemed: candy_machine.items_redeemed,
        features: candy_machine.features,
        mint_authority: transform_optional_pubkeys(candy_machine.mint_authority),
        candy_guard,
    })
}
