use crate::{
    dao::{
        candy_guard_group::Model as GuardGroupModel,
        candy_machine_data::Model,
        generated::sea_orm_active_enums::{EndSettingType, WhitelistMintMode},
    },
    rpc::{
        AllowList, BotTax, ConfigLineSettings, EndSettings, FreezeInfo, Gatekeeper, GuardSet,
        HiddenSettings, Lamports, LiveDate, MintLimit, NftPayment, SplToken, ThirdPartySigner,
        WhitelistMintSettings,
    },
};

// All of these helpers are essentially transforming the separate Option<> fields
// into their optionable struct types to be returned from rpc

pub fn get_end_settings(
    end_setting_number: Option<i64>,
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
    spl_token_amount: Option<i64>,
    spl_token_mint: Option<Vec<u8>>,
    spl_token_destination_ata: Option<Vec<u8>>,
) -> Option<SplToken> {
    if let (Some(spl_token_amount), Some(spl_token_mint), Some(spl_token_destination_ata)) =
        (spl_token_amount, spl_token_mint, spl_token_destination_ata)
    {
        Some(SplToken {
            amount: spl_token_amount,
            token_mint: bs58::encode(spl_token_mint).into_string(),
            destination_ata: bs58::encode(spl_token_destination_ata).into_string(),
        })
    } else {
        None
    }
}

pub fn get_nft_payment(
    nft_payment_destination: Option<Vec<u8>>,
    nft_payment_required_collection: Option<Vec<u8>>,
) -> Option<NftPayment> {
    if let (Some(nft_payment_destination), Some(nft_payment_required_collection)) =
        (nft_payment_destination, nft_payment_required_collection)
    {
        Some(NftPayment {
            destination: bs58::encode(nft_payment_destination).into_string(),
            required_collection: bs58::encode(nft_payment_required_collection).into_string(),
        })
    } else {
        None
    }
}

pub fn get_freeze_info(
    allow_thaw: Option<bool>,
    frozen_count: Option<i64>,
    freeze_time: Option<i64>,
    freeze_fee: Option<i64>,
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

pub fn get_whitelist_settings(
    whitelist_mode: Option<WhitelistMintMode>,
    whitelist_mint: Option<Vec<u8>>,
    whitelist_presale: Option<bool>,
    whitelist_discount_price: Option<i64>,
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
    gatekeeper_network: Option<Vec<u8>>,
    gatekeeper_expire_on_use: Option<bool>,
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

pub fn get_hidden_settings(
    name: Option<String>,
    uri: Option<String>,
    hash: Option<Vec<u8>>,
) -> Option<HiddenSettings> {
    if let (Some(name), Some(uri), Some(hash)) = (name, uri, hash) {
        Some(HiddenSettings {
            name,
            uri,
            hash: bs58::encode(hash).into_string(),
        })
    } else {
        None
    }
}

pub fn get_lamports(amount: Option<i64>, destination: Option<Vec<u8>>) -> Option<Lamports> {
    if let (Some(amount), Some(destination)) = (amount, destination) {
        Some(Lamports {
            amount,
            destination: bs58::encode(destination).into_string(),
        })
    } else {
        None
    }
}

pub fn get_allow_list(merkle_root: Option<Vec<u8>>) -> Option<AllowList> {
    if let Some(merkle_root) = merkle_root {
        Some(AllowList {
            merkle_root: bs58::encode(merkle_root).into_string(),
        })
    } else {
        None
    }
}

pub fn get_third_party_signer(signer_key: Option<Vec<u8>>) -> Option<ThirdPartySigner> {
    if let Some(signer) = signer_key {
        Some(ThirdPartySigner {
            signer_key: bs58::encode(signer).into_string(),
        })
    } else {
        None
    }
}

pub fn get_config_line_settings(
    is_sequential: Option<bool>,
    name_length: Option<i32>,
    prefix_name: Option<String>,
    prefix_uri: Option<String>,
    uri_length: Option<i32>,
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

pub fn get_live_date(live_date: Option<i64>) -> Option<LiveDate> {
    if live_date.is_some() {
        Some(LiveDate { date: live_date })
    } else {
        None
    }
}

pub fn get_bot_tax(lamports: Option<i64>, last_instruction: Option<bool>) -> Option<BotTax> {
    if let (Some(lamports), Some(last_instruction)) = (lamports, last_instruction) {
        Some(BotTax {
            lamports,
            last_instruction,
        })
    } else {
        None
    }
}

pub fn get_candy_guard_group(group: &GuardGroupModel) -> GuardSet {
    let gatekeeper = get_gatekeeper(
        group.gatekeeper_network.clone(),
        group.gatekeeper_expire_on_use,
    );

    let third_party_signer = get_third_party_signer(group.third_party_signer_key.clone());
    let allow_list = get_allow_list(group.allow_list_merkle_root.clone());
    let nft_payment = get_nft_payment(
        group.nft_payment_destination.clone(),
        group.nft_payment_required_collection.clone(),
    );

    // TODO fix later P-682
    // let mint_limit = get_mint_limit(group.mint_limit_id, group.mint_limit_limit);

    // let bot_tax = get_bot_tax(group.bot_tax_lamports, group.bot_tax_last_instruction);

    GuardSet {
        bot_tax: None,
        third_party_signer,
        gatekeeper,
        allow_list,
        mint_limit: None,
        nft_payment,
    }
}
pub fn get_candy_machine_data(
    candy_machine_data: Model,
) -> (
    Option<WhitelistMintSettings>,
    Option<HiddenSettings>,
    Option<ConfigLineSettings>,
    Option<EndSettings>,
    Option<Gatekeeper>,
) {
    let data_end_settings = get_end_settings(
        candy_machine_data.end_setting_number,
        candy_machine_data.end_setting_type,
    );

    let data_hidden_settings = get_hidden_settings(
        candy_machine_data.hidden_settings_name,
        candy_machine_data.hidden_settings_uri,
        candy_machine_data.hidden_settings_hash,
    );
    let data_gatekeeper = get_gatekeeper(
        candy_machine_data.gatekeeper_network,
        candy_machine_data.gatekeeper_expire_on_use,
    );
    let data_whitelist_mint_settings = get_whitelist_settings(
        candy_machine_data.whitelist_mode,
        candy_machine_data.whitelist_mint,
        candy_machine_data.whitelist_presale,
        candy_machine_data.whitelist_discount_price,
    );

    let data_config_line_settings = get_config_line_settings(
        candy_machine_data.config_line_settings_is_sequential,
        candy_machine_data.config_line_settings_name_length,
        candy_machine_data.config_line_settings_prefix_name,
        candy_machine_data.config_line_settings_prefix_uri,
        candy_machine_data.config_line_settings_uri_length,
    );

    (
        data_whitelist_mint_settings,
        data_hidden_settings,
        data_config_line_settings,
        data_end_settings,
        data_gatekeeper,
    )
}
