use crate::{
    dao::generated::{
        candy_guard_group::Model as GuardGroupModel,
        candy_machine_data::Model,
        sea_orm_active_enums::{EndSettingType, WhitelistMintMode},
    },
    rpc::{
        AddressGate, AllowList, BotTax, ConfigLineSettings, EndDate, EndSettings, FreezeInfo,
        FreezeSolPayment, FreezeTokenPayment, Gatekeeper, GuardSet, HiddenSettings, MintLimit,
        NftBurn, NftGate, NftPayment, RedeemedAmount, SolPayment, StartDate, ThirdPartySigner,
        TokenBurn, TokenGate, TokenPayment, WhitelistMintSettings,
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
            number: end_setting_number as u64,
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
    mint_limit_id: Option<i16>,
    mint_limit_limit: Option<i16>,
) -> Option<MintLimit> {
    if let (Some(mint_limit_id), Some(mint_limit_limit)) = (mint_limit_id, mint_limit_limit) {
        Some(MintLimit {
            id: mint_limit_id as u8,
            limit: mint_limit_limit as u16,
        })
    } else {
        None
    }
}

pub fn get_start_date(start_date: Option<i64>) -> Option<StartDate> {
    if let Some(start_date) = start_date {
        Some(StartDate {
            date: start_date as u64,
        })
    } else {
        None
    }
}

pub fn get_end_date(end_date: Option<i64>) -> Option<EndDate> {
    if let Some(end_date) = end_date {
        Some(EndDate {
            date: end_date as u64,
        })
    } else {
        None
    }
}

pub fn get_bot_tax(
    bot_tax_lamports: Option<i64>,
    bot_tax_last_instruction: Option<bool>,
) -> Option<BotTax> {
    if let (Some(bot_tax_lamports), Some(bot_tax_last_instruction)) =
        (bot_tax_lamports, bot_tax_last_instruction)
    {
        Some(BotTax {
            lamports: bot_tax_lamports as u64,
            last_instruction: bot_tax_last_instruction,
        })
    } else {
        None
    }
}

pub fn get_sol_payment(
    sol_payment_lamports: Option<i64>,
    sol_payment_destination: Option<Vec<u8>>,
) -> Option<SolPayment> {
    if let (Some(sol_payment_lamports), Some(sol_payment_destination)) =
        (sol_payment_lamports, sol_payment_destination)
    {
        Some(SolPayment {
            lamports: sol_payment_lamports as u64,
            destination: bs58::encode(sol_payment_destination).into_string(),
        })
    } else {
        None
    }
}

pub fn get_address_gate(address_gate_address: Option<Vec<u8>>) -> Option<AddressGate> {
    if let Some(address_gate_address) = address_gate_address {
        Some(AddressGate {
            address: bs58::encode(address_gate_address).into_string(),
        })
    } else {
        None
    }
}

pub fn get_redeemed_amount(redeemed_amount_maximum: Option<i64>) -> Option<RedeemedAmount> {
    if let Some(redeemed_amount_maximum) = redeemed_amount_maximum {
        Some(RedeemedAmount {
            maximum: redeemed_amount_maximum as u64,
        })
    } else {
        None
    }
}

pub fn get_freeze_sol_payment(
    freeze_sol_payment_lamports: Option<i64>,
    freeze_sol_payment_destination: Option<Vec<u8>>,
) -> Option<FreezeSolPayment> {
    if let (Some(freeze_sol_payment_lamports), Some(freeze_sol_payment_destination)) =
        (freeze_sol_payment_lamports, freeze_sol_payment_destination)
    {
        Some(FreezeSolPayment {
            lamports: freeze_sol_payment_lamports as u64,
            destination: bs58::encode(freeze_sol_payment_destination).into_string(),
        })
    } else {
        None
    }
}

pub fn get_token_gate(
    token_gate_amount: Option<i64>,
    token_gate_mint: Option<Vec<u8>>,
) -> Option<TokenGate> {
    if let (Some(token_gate_amount), Some(token_gate_mint)) = (token_gate_amount, token_gate_mint) {
        Some(TokenGate {
            amount: token_gate_amount as u64,
            mint: bs58::encode(token_gate_mint).into_string(),
        })
    } else {
        None
    }
}

pub fn get_nft_gate(nft_gate_required_collection: Option<Vec<u8>>) -> Option<NftGate> {
    if let Some(nft_gate_required_collection) = nft_gate_required_collection {
        Some(NftGate {
            required_collection: bs58::encode(nft_gate_required_collection).into_string(),
        })
    } else {
        None
    }
}

pub fn get_token_burn(
    token_burn_amount: Option<i64>,
    token_burn_mint: Option<Vec<u8>>,
) -> Option<TokenBurn> {
    if let (Some(token_burn_amount), Some(token_burn_mint)) = (token_burn_amount, token_burn_mint) {
        Some(TokenBurn {
            amount: token_burn_amount as u64,
            mint: bs58::encode(token_burn_mint).into_string(),
        })
    } else {
        None
    }
}

pub fn get_nft_burn(nft_gate_required_collection: Option<Vec<u8>>) -> Option<NftBurn> {
    if let Some(nft_gate_required_collection) = nft_gate_required_collection {
        Some(NftBurn {
            required_collection: bs58::encode(nft_gate_required_collection).into_string(),
        })
    } else {
        None
    }
}

pub fn get_token_payment(
    token_payment_amount: Option<i64>,
    token_payment_mint: Option<Vec<u8>>,
    token_payment_destination_ata: Option<Vec<u8>>,
) -> Option<TokenPayment> {
    if let (
        Some(token_payment_amount),
        Some(token_payment_mint),
        Some(token_payment_destination_ata),
    ) = (
        token_payment_amount,
        token_payment_mint,
        token_payment_destination_ata,
    ) {
        Some(TokenPayment {
            amount: token_payment_amount as u64,
            mint: bs58::encode(token_payment_mint).into_string(),
            destination_ata: bs58::encode(token_payment_destination_ata).into_string(),
        })
    } else {
        None
    }
}

pub fn get_freeze_token_payment(
    freeze_token_payment_amount: Option<i64>,
    freeze_token_payment_mint: Option<Vec<u8>>,
    freeze_token_payment_destination_ata_ata: Option<Vec<u8>>,
) -> Option<FreezeTokenPayment> {
    if let (
        Some(freeze_token_payment_amount),
        Some(freeze_token_payment_mint),
        Some(freeze_token_payment_destination_ata_ata),
    ) = (
        freeze_token_payment_amount,
        freeze_token_payment_mint,
        freeze_token_payment_destination_ata_ata,
    ) {
        Some(FreezeTokenPayment {
            amount: freeze_token_payment_amount as u64,
            mint: bs58::encode(freeze_token_payment_mint).into_string(),
            destination_ata: bs58::encode(freeze_token_payment_destination_ata_ata).into_string(),
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

    let sol_payment = get_sol_payment(
        group.sol_payment_lamports,
        group.sol_payment_destination.clone(),
    );
    let mint_limit = get_mint_limit(group.mint_limit_id, group.mint_limit_limit);
    let bot_tax = get_bot_tax(group.bot_tax_lamports, group.bot_tax_last_instruction);
    let start_date = get_start_date(group.start_date);
    let end_date = get_end_date(group.end_date);
    let address_gate = get_address_gate(group.address_gate_address.clone());
    let redeemed_amount = get_redeemed_amount(group.redeemed_amount_maximum);
    let freeze_sol_payment = get_freeze_sol_payment(
        group.freeze_sol_payment_lamports,
        group.freeze_sol_payment_destination.clone(),
    );
    let token_gate = get_token_gate(group.token_gate_amount, group.token_gate_mint.clone());
    let nft_gate = get_nft_gate(group.nft_gate_required_collection.clone());
    let token_burn = get_token_burn(group.token_burn_amount, group.token_burn_mint.clone());
    let nft_burn = get_nft_burn(group.nft_burn_required_collection.clone());
    let token_payment = get_token_payment(
        group.token_payment_amount,
        group.token_payment_mint.clone(),
        group.token_payment_destination_ata.clone(),
    );
    let freeze_token_payment = get_freeze_token_payment(
        group.freeze_token_payment_amount,
        group.freeze_token_payment_mint.clone(),
        group.freeze_token_payment_destination_ata.clone(),
    );

    GuardSet {
        bot_tax,
        third_party_signer,
        gatekeeper,
        allow_list,
        mint_limit,
        nft_payment,
        sol_payment,
        start_date,
        end_date,
        address_gate,
        redeemed_amount,
        freeze_sol_payment,
        token_gate,
        nft_gate,
        token_burn,
        nft_burn,
        token_payment,
        freeze_token_payment,
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
