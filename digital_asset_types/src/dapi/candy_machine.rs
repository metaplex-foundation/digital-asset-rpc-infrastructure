use crate::dao::prelude::{CandyGuard, CandyGuardGroup, CandyMachine, CandyMachineData};
use crate::dao::{
    candy_guard, candy_guard_group, candy_machine, candy_machine_creators, candy_machine_data,
};
use crate::rpc::{
    AllowList, BotTax, CandyGuard as RpcCandyGuard, CandyGuardData,
    CandyMachine as RpcCandyMachine, CandyMachineData as RpcCandyMachineData, ConfigLineSettings,
    Creator, EndSettings, FreezeInfo, Gatekeeper, GuardSet, HiddenSettings, Lamports, LiveDate,
    MintLimit, NftPayment, SplToken, ThirdPartySigner, WhitelistMintSettings,
};

use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};

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

pub fn get_hidden_settings(hidden_settings: &Option<HiddenSettings>) -> Option<HiddenSettings> {
    if hidden_settings.is_some() {
        hidden_settings
    } else {
        None
    }
}

pub fn get_gatekeeper(gatekeeper: &Option<Gatekeeper>) -> Option<Gatekeeper> {
    if let Some(gatekeeper) = gatekeeper {
        Some(Gatekeeper {
            gatekeeper_network: bs58::encode(gatekeeper.network).into_string(),
            expire_on_use: gatekeeper.expire_on_use,
        })
    } else {
        None
    }
}

pub fn get_whitelist_settings(
    whitelist_settings: &Option<WhitelistMintSettings>,
) -> Option<WhitelistMintSettings> {
    if let Some(whitelist_settings) = whitelist_settings {
        Some(WhitelistMintSettings {
            mode: whitelist_settings.mode,
            mint: bs58::encode(whitelist_settings.mint).into_string(),
            presale: whitelist_settings.presale,
            discount_price: whitelist_settings.discount_price,
        })
    } else {
        None
    }
}

pub fn get_bot_tax(bot_tax: &Option<BotTax>) -> Option<BotTax> {
    if bot_tax.is_some() {
        bot_tax
    } else {
        None
    }
}

// TODO move all these ^ function to one big match function
pub fn get_lamports(lamports: &Option<Lamports>) -> Option<Lamports> {
    if lamports.is_some() {
        lamports
    } else {
        None
    }
}

pub fn get_allow_list(allow_list: &Option<AllowList>) -> Option<AllowList> {
    if allow_list.is_some() {
        allow_list
    } else {
        None
    }
}

pub fn get_mint_limit(mint_limit: &Option<MintLimit>) -> Option<MintLimit> {
    if mint_limit.is_some() {
        mint_limit
    } else {
        None
    }
}

pub fn get_nft_payment(nft_payment: &Option<NftPayment>) -> Option<NftPayment> {
    if nft_payment.is_some() {
        nft_payment
    } else {
        None
    }
}

pub fn get_live_date(live_date: &Option<LiveDate>) -> Option<LiveDate> {
    if live_date.is_some() {
        live_date
    } else {
        None
    }
}

pub fn get_third_party_signer(signer: &Option<ThirdPartySigner>) -> Option<ThirdPartySigner> {
    if signer.is_some() {
        signer
    } else {
        None
    }
}

pub fn get_spl_token(spl_token: &Option<SplToken>) -> Option<SplToken> {
    if spl_token.is_some() {
        spl_token
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

        let default_set = candy_guard_group
            .into_iter()
            .find(|&group| group.label.is_none())
            .map(|&group| {
                let gatekeeper = get_gatekeeper(group.gatekeeper);
                let bot_tax = get_bot_tax(group.bot_tax);
                let lamports = get_lamports(group.lamports);
                let spl_token = get_spl_token(group.spl_token);
                let live_date = get_live_date(group.live_date);
                let third_party_signer = get_third_party_signer(group.third_party_signer);
                let allow_list = get_allow_list(group.allow_list);
                let mint_limit = get_mint_limit(group.mint_limit);
                let nft_payment = get_nft_payment(group.nft_payment);
                let whitelist_settings = get_whitelist_settings(group.whitelist_mint_settings);

                GuardSet {
                    bot_tax,
                    lamports,
                    spl_token,
                    live_date,
                    third_party_signer,
                    whitelist: whitelist_settings,
                    gatekeeper,
                    end_settings: group.end_settings,
                    allow_list,
                    mint_limit,
                    nft_payment,
                }
            })
            .unwrap();

        let groups = Vec::new();
        for group in candy_guard_group
            .iter()
            .filter(|group| group.label.is_some())
        {
            let gatekeeper = get_gatekeeper(group.gatekeeper);
            let bot_tax = get_bot_tax(group.bot_tax);
            let lamports = get_lamports(group.lamports);
            let spl_token = get_spl_token(group.spl_token);
            let live_date = get_live_date(group.live_date);
            let third_party_signer = get_third_party_signer(group.third_party_signer);
            let allow_list = get_allow_list(group.allow_list);
            let mint_limit = get_mint_limit(group.mint_limit);
            let nft_payment = get_nft_payment(group.nft_payment);
            let whitelist_settings = get_whitelist_settings(group.whitelist_mint_settings);

            groups.push(GuardSet {
                bot_tax,
                lamports,
                spl_token,
                live_date,
                third_party_signer,
                whitelist: whitelist_settings,
                gatekeeper,
                end_settings: group.end_settings,
                allow_list,
                mint_limit,
                nft_payment,
            })
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
    let data_gatekeeper = get_gatekeeper(candy_machine_data.gatekeeper);
    let data_whitelist_mint_settings =
        get_whitelist_settings(candy_machine_data.whitelist_mint_settings);

    // TODO figure out which option types were not saved in db asvec u8 when they should have been
    // TODO figure out which ones need bs58 encoding added to them
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
            config_line_settings: candy_machine_data.config_line_settings,
            hidden_settings: data_hidden_settings,
            end_settings: candy_machine_data.end_settings,
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
