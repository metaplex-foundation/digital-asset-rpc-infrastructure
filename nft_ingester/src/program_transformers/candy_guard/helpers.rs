use mpl_candy_guard::guards::{
    AddressGate, AllowList, BotTax, EndDate, FreezeSolPayment, FreezeTokenPayment, Gatekeeper,
    MintLimit, NftBurn, NftGate, NftPayment, RedeemedAmount, SolPayment, StartDate,
    ThirdPartySigner, TokenBurn, TokenGate, TokenPayment,
};

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

pub fn get_sol_payment(sol_payment: Option<SolPayment>) -> (Option<u64>, Option<Vec<u8>>) {
    if let Some(sol_payment) = sol_payment {
        (
            Some(sol_payment.lamports),
            Some(sol_payment.destination.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    }
}

pub fn get_start_date(start_date: Option<StartDate>) -> Option<i64> {
    if let Some(start_date) = start_date {
        Some(start_date.date)
    } else {
        None
    }
}

pub fn get_end_date(end_date: Option<EndDate>) -> Option<i64> {
    if let Some(end_date) = end_date {
        Some(end_date.date)
    } else {
        None
    }
}

pub fn get_redeemed_amount(redeemed_amount: Option<RedeemedAmount>) -> Option<i64> {
    if let Some(redeemed_amount) = redeemed_amount {
        Some(redeemed_amount.maximum)
    } else {
        None
    }
}

pub fn get_address_gate(address_gate: Option<AddressGate>) -> Option<Vec<u8>> {
    if let Some(address_gate) = address_gate {
        Some(address_gate.address.to_bytes().to_vec())
    } else {
        None
    }
}

pub fn get_freeze_sol_payment(
    freeze_sol_payment: Option<FreezeSolPayment>,
) -> (Option<i64>, Option<Vec<u8>>) {
    if let Some(freeze_sol_payment) = freeze_sol_payment {
        (
            Some(freeze_sol_payment.lamports),
            Some(freeze_sol_payment.destination.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    }
}

pub fn get_token_gate(token_gate: Option<TokenGate>) -> (Option<u64>, Option<Vec<u8>>) {
    if let Some(token_gate) = token_gate {
        (
            Some(token_gate.amount),
            Some(token_gate.mint.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    }
}

pub fn get_nft_gate(nft_gate: Option<NftGate>) -> Option<Vec<u8>> {
    if let Some(nft_gate) = nft_gate {
        Some(nft_gate.required_collection.to_bytes().to_vec())
    } else {
        None
    }
}

pub fn get_token_burn(token_burn: Option<TokenBurn>) -> (Option<u64>, Option<Vec<u8>>) {
    if let Some(token_burn) = token_burn {
        (
            Some(token_burn.amount),
            Some(token_burn.mint.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    }
}

pub fn get_nft_burn(nft_burn: Option<NftBurn>) -> Option<Vec<u8>> {
    if let Some(nft_burn) = nft_burn {
        Some(nft_burn.required_collection.to_bytes().to_vec())
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

pub fn get_token_payment(token_payment: Option<TokenPayment>) {
    if let Some(token_payment) = token_payment {
        (
            Some(token_payment.amount),
            Some(token_payment.mint.to_bytes().to_vec()),
            Some(token_payment.destination_ata.to_bytes().to_vec()),
        )
    } else {
        (None, None, None)
    }
}

pub fn get_allow_list(allow_list: Option<AllowList>) -> Option<Vec<u8>> {
    if let Some(allow_list) = allow_list {
        Some(allow_list.merkle_root.to_vec())
    } else {
        None
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
        (
            Some(bot_tax.lamports.try_into().unwrap()),
            Some(bot_tax.last_instruction),
        )
    } else {
        (None, None)
    }
}

pub fn get_freeze_token_payment(freeze_token_payment: Option<FreezeTokenPayment>) {
    if let Some(freeze_token_payment) = freeze_token_payment {
        (
            Some(freeze_token_payment.amount),
            Some(freeze_token_payment.mint.to_bytes().to_vec()),
            Some(freeze_token_payment.destination.to_bytes().to_vec()),
        )
    } else {
        (None, None, None)
    }
}
