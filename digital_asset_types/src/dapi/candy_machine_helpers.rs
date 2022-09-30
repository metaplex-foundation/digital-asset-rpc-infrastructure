use crate::{
    dao::sea_orm_active_enums::{EndSettingType, WhitelistMintMode},
    rpc::{
        AllowList, BotTax, ConfigLineSettings, EndSettings, FreezeInfo, Gatekeeper, HiddenSettings,
        Lamports, LiveDate, MintLimit, NftPayment, SplToken, ThirdPartySigner,
        WhitelistMintSettings,
    },
};

// TODO make get all settings methods and get all guards methods using these

// All of these helpers are essentially transforming the separate Option<> fields
// into their optionable struct types to be returned from rpc

pub fn get_end_settings(
    end_setting_number: Option<u64>,
    end_setting_type: Option<EndSettingType>,
) -> Option<EndSettings> {
    if let (Some(end_setting_number), Some(end_setting_type)) =
        (end_setting_number, end_setting_type)
    {
        Some(EndSettings {
            end_setting_type,
            number: end_setting_number,
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

pub fn get_hidden_settings(hidden_settings: &Option<HiddenSettings>) -> Option<HiddenSettings> {
    if hidden_settings.is_some() {
        // TODO what to do with hash here ? turn into string? or return as [u8]
        hidden_settings
    } else {
        None
    }
}

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

pub fn get_third_party_signer(signer: &Option<ThirdPartySigner>) -> Option<ThirdPartySigner> {
    if let Some(signer) = signer {
        Some(ThirdPartySigner {
            signer_key: bs58::encode(signer.signer_key).to_string(),
        })
    } else {
        None
    }
}

pub fn get_config_line_settings(
    is_sequential: Option<bool>,
    name_length: Option<u32>,
    prefix_name: Option<String>,
    prefix_uri: Option<String>,
    uri_length: Option<u32>,
) -> Option<ConfigLineSettings> {
    if let (
        Some(is_sequential),
        Some(name_length),
        Some(prefix_name),
        Some(prefix_uri),
        Some(uri_length),
    ) = (
        is_sequential,
        name_length,
        prefix_name,
        prefix_uri,
        uri_length,
    ) {
        Some(ConfigLineSettings {
            prefix_name,
            name_length,
            prefix_uri,
            uri_length,
            is_sequential,
        })
    } else {
        None
    }
}

pub fn get_mint_limit(
    mint_limit_id: Option<u8>,
    mint_limit_limit: Option<u16>,
) -> Option<MintLimit> {
    if let (Some(mint_limit_id), Some(mint_limit_limit)) = (mint_limit_id, mint_limit_limit) {
        Some(MintLimit {
            id: mint_limit_id,
            limit: mint_limit_limit,
        })
    } else {
        None
    }
}

pub fn get_live_date(live_date: Option<u64>) -> Option<LiveDate> {
    if let Some(date) = live_date {
        Some(LiveDate { date: live_date })
    } else {
        None
    }
}

pub fn get_bot_tax(lamports: Option<u64>, last_instruction: Option<bool>) -> Option<BotTax> {
    if let (Some(lamports), Some(last_instruction)) = (lamports, last_instruction) {
        Some(BotTax {
            lamports,
            last_instruction,
        })
    } else {
        None
    }
}
