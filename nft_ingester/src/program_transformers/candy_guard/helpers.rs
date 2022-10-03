use crate::IngesterError;
use blockbuster::programs::bubblegum::ChangeLogEvent;
use digital_asset_types::dao::generated::{backfill_items, cl_items};
use mpl_candy_guard::guards::{
    AllowList, BotTax, GuardSet, Lamports, LiveDate, MintLimit, NftPayment, SplToken,
    ThirdPartySigner,
};
use mpl_candy_machine_core::ConfigLineSettings;
use sea_orm::{entity::*, query::*, sea_query::OnConflict, DatabaseTransaction, DbBackend};

pub enum EndSettingType {
    Date,
    Amount,
}

pub fn get_nft_payment(nft_payment: Option<NftPayment>) -> (Option<bool>, Option<Vec<u8, Global>>) {
    if let Some(nft_payment) = candy_guard_data.nft_payment {
        (
            Some(nft_payment.nft_payment_burn),
            Some(
                nft_payment
                    .nft_payment_required_collection
                    .to_bytes()
                    .to_vec(),
            ),
        )
    } else {
        None
    }
}

pub fn get_third_party_signer(
    third_party_signer: Option<ThirdPartySigner>,
) -> Option<Vec<u8, Global>> {
    if let Some(third_party_signer) = third_party_signer {
        Some(third_party_signer.signer_key.to_bytes().to_vec())
    } else {
        None
    }
}

pub fn get_live_date(live_date: Option<LiveDate>) -> Option<i64> {
    if let Some(live_date) = live_date {
        live_date.date
    } else {
        None
    }
}
pub fn get_allow_list(allow_list: Option<AllowList>) -> Option<Vec<u8>> {
    if let Some(allow_list) = allow_list {
        Some(allow_list.merkle_root.to_vec())
    } else {
        None
    }
}

pub fn get_mint_limit(mint_limit: Option<MintLimit>) -> (Option<u8>, Option<u16>) {
    if let Some(mint_limit) = candy_guard_data.mint_limit {
        (
            Some(mint_limit.mint_limit_id),
            Some(mint_limit.mint_limit_limit),
        )
    } else {
        (None, None)
    }
}

pub fn get_spl_token(
    spl_token: Option<SplToken>,
) -> (
    Option<u64>,
    Option<Vec<u8, Global>>,
    Option<Vec<u8, Global>>,
) {
    if let Some(spl_token) = spl_token {
        (
            Some(spl_token.amount),
            Some(spl_token.token_mint.to_bytes().to_vec()),
            Some(spl_token.destination_ata.to_bytes().to_vec()),
        )
    } else {
        (None, None, None)
    }
}

pub fn get_lamports(lamports: Option<Lamports>) -> (Option<u64>, Option<Vec<u8, Global>>) {
    if let Some(lamports) = lamports {
        (
            Some(lamports.amount),
            Some(lamports.destination.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    }
}

pub fn get_whitelist_settings(
    whitelist_mint_settings: Option<Whitelist>,
) -> Option<Option<WhitelistMintMode>, Option<bool>, Option<Vec<u8, Global>>, Option<u64>> {
    if let Some(whitelist) = whitelist_mint_settings {
        (
            Some(whitelist.mode),
            Some(whitelist.presale),
            Some(whitelist.mint.to_bytes().to_vec()),
            whitelist.discount_price,
        )
    } else {
        (None, None, None, None)
    }
}

pub fn get_gatekeeper(gatekeeper: Option<Gatekeeper>) -> (Option<bool>, Option<Vec<u8, Global>>) {
    if let Some(gatekeeper) = gatekeeper {
        (
            Some(gatekeeper.expire_on_use),
            Some(gatekeeper.gatekeeper_network.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    }
}

pub fn get_bot_tax(bot_tax: Option<BotTax>) -> (Option<u64>, Option<bool>) {
    if let Some(bot_tax) = bot_tax {
        (Some(bot_tax.lamports), Some(bot_tax.last_instruction))
    } else {
        (None, None)
    }
}

pub fn get_end_settings(
    end_settings: Option<EndSettings>,
) -> (Option<EndSettingType>, Option<u64>) {
    if let Some(end_settings) = end_settings {
        (
            Some(end_settings.end_setting_type),
            Some(end_settings.number),
        )
    } else {
        (None, None)
    }
}
