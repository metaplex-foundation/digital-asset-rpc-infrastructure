use mpl_candy_guard::guards::{
    AllowList, BotTax, Gatekeeper, MintLimit, NftPayment, ThirdPartySigner,
};

pub enum EndSettingType {
    Date,
    Amount,
}

pub fn get_nft_payment(nft_payment: Option<NftPayment>) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    if let Some(nft_payment) = nft_payment {
        (
            Some(nft_payment.destination.to_bytes().to_vec()),
            Some(nft_payment.required_collection.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    }
}

pub fn get_third_party_signer(third_party_signer: Option<ThirdPartySigner>) -> Option<Vec<u8>> {
    if let Some(third_party_signer) = third_party_signer {
        Some(third_party_signer.signer_key.to_bytes().to_vec())
    } else {
        None
    }
}

// pub fn get_live_date(live_date: Option<LiveDate>) -> Option<i64> {
//     if let Some(live_date) = live_date {
//         live_date.date
//     } else {
//         None
//     }
// }
pub fn get_allow_list(allow_list: Option<AllowList>) -> Option<Vec<u8>> {
    if let Some(allow_list) = allow_list {
        Some(allow_list.merkle_root.to_vec())
    } else {
        None
    }
}

pub fn get_mint_limit(mint_limit: Option<MintLimit>) -> (Option<u8>, Option<u16>) {
    if let Some(mint_limit) = mint_limit {
        (Some(mint_limit.id), Some(mint_limit.limit))
    } else {
        (None, None)
    }
}

pub fn get_gatekeeper(gatekeeper: Option<Gatekeeper>) -> (Option<bool>, Option<Vec<u8>>) {
    if let Some(gatekeeper) = gatekeeper {
        (
            Some(gatekeeper.expire_on_use),
            Some(gatekeeper.gatekeeper_network.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    }
}

pub fn get_bot_tax(bot_tax: Option<BotTax>) -> (Option<i64>, Option<bool>) {
    if let Some(bot_tax) = bot_tax {
        (Some(bot_tax.lamports.try_into().unwrap()), Some(bot_tax.last_instruction))
    } else {
        (None, None)
    }
}

// pub fn get_end_settings(
//     end_settings: Option<EndSettings>,
// ) -> (Option<EndSettingType>, Option<i64>) {
//     if let Some(end_settings) = end_settings {
//         (
//             Some(end_settings.end_setting_type),
//             Some(end_settings.number),
//         )
//     } else {
//         (None, None)
//     }
// }
