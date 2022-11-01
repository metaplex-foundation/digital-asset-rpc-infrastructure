use mpl_candy_guard::{
    guards::{
        AddressGate, AllowList, BotTax, EndDate, FreezeSolPayment, FreezeTokenPayment, Gatekeeper,
        MintLimit, NftBurn, NftGate, NftPayment, RedeemedAmount, SolPayment, StartDate,
        ThirdPartySigner, TokenBurn, TokenGate, TokenPayment,
    },
    state::GuardSet,
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

pub fn get_sol_payment(sol_payment: Option<SolPayment>) -> (Option<i64>, Option<Vec<u8>>) {
    if let Some(sol_payment) = sol_payment {
        (
            Some(sol_payment.lamports as i64),
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
        Some(redeemed_amount.maximum as i64)
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
            Some(freeze_sol_payment.lamports as i64),
            Some(freeze_sol_payment.destination.to_bytes().to_vec()),
        )
    } else {
        (None, None)
    }
}

pub fn get_token_gate(token_gate: Option<TokenGate>) -> (Option<i64>, Option<Vec<u8>>) {
    if let Some(token_gate) = token_gate {
        (
            Some(token_gate.amount as i64),
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

pub fn get_token_burn(token_burn: Option<TokenBurn>) -> (Option<i64>, Option<Vec<u8>>) {
    if let Some(token_burn) = token_burn {
        (
            Some(token_burn.amount as i64),
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

pub fn get_mint_limit(mint_limit: Option<MintLimit>) -> (Option<i16>, Option<i16>) {
    if let Some(mint_limit) = mint_limit {
        (Some(mint_limit.id as i16), Some(mint_limit.limit as i16))
    } else {
        (None, None)
    }
}

pub fn get_token_payment(
    token_payment: Option<TokenPayment>,
) -> (Option<i64>, Option<Vec<u8>>, Option<Vec<u8>>) {
    if let Some(token_payment) = token_payment {
        (
            Some(token_payment.amount as i64),
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
            Some(bot_tax.lamports as i64),
            Some(bot_tax.last_instruction),
        )
    } else {
        (None, None)
    }
}

pub fn get_freeze_token_payment(
    freeze_token_payment: Option<FreezeTokenPayment>,
) -> (Option<i64>, Option<Vec<u8>>, Option<Vec<u8>>) {
    if let Some(freeze_token_payment) = freeze_token_payment {
        (
            Some(freeze_token_payment.amount as i64),
            Some(freeze_token_payment.mint.to_bytes().to_vec()),
            Some(freeze_token_payment.destination_ata.to_bytes().to_vec()),
        )
    } else {
        (None, None, None)
    }
}

pub struct DBGuardSet {
    pub bot_tax_lamports: Option<i64>,
    pub bot_tax_last_instruction: Option<bool>,
    pub start_date: Option<i64>,
    pub end_date: Option<i64>,
    pub third_party_signer_key: Option<Vec<u8>>,
    pub nft_payment_destination: Option<Vec<u8>>,
    pub nft_payment_required_collection: Option<Vec<u8>>,
    pub mint_limit_id: Option<i16>,
    pub mint_limit_limit: Option<i16>,
    pub gatekeeper_network: Option<Vec<u8>>,
    pub gatekeeper_expire_on_use: Option<bool>,
    pub sol_payment_lamports: Option<i64>,
    pub sol_payment_destination: Option<Vec<u8>>,
    pub redeemed_amount_maximum: Option<i64>,
    pub address_gate_address: Option<Vec<u8>>,
    pub freeze_sol_payment_lamports: Option<i64>,
    pub freeze_sol_payment_destination: Option<Vec<u8>>,
    pub token_gate_amount: Option<i64>,
    pub token_gate_mint: Option<Vec<u8>>,
    pub nft_gate_required_collection: Option<Vec<u8>>,
    pub token_burn_amount: Option<i64>,
    pub token_burn_mint: Option<Vec<u8>>,
    pub nft_burn_required_collection: Option<Vec<u8>>,
    pub token_payment_amount: Option<i64>,
    pub token_payment_mint: Option<Vec<u8>>,
    pub token_payment_destination_ata: Option<Vec<u8>>,
    pub allow_list_merkle_root: Option<Vec<u8>>,
    pub freeze_token_payment_amount: Option<i64>,
    pub freeze_token_payment_mint: Option<Vec<u8>>,
    pub freeze_token_payment_destination_ata: Option<Vec<u8>>,
}

pub fn get_all_guards(guard_set: GuardSet) -> DBGuardSet {
    let (
        freeze_token_payment_amount,
        freeze_token_payment_mint,
        freeze_token_payment_destination_ata,
    ) = get_freeze_token_payment(guard_set.freeze_token_payment);
    let (bot_tax_lamports, bot_tax_last_instruction) = get_bot_tax(guard_set.bot_tax);
    let (sol_payment_lamports, sol_payment_destination) = get_sol_payment(guard_set.sol_payment);
    let (redeemed_amount_maximum) = get_redeemed_amount(guard_set.redeemed_amount);
    let (address_gate_address) = get_address_gate(guard_set.address_gate);
    let (nft_payment_destination, nft_payment_required_collection) =
        get_nft_payment(guard_set.nft_payment);
    let start_date = get_start_date(guard_set.start_date);
    let end_date = get_end_date(guard_set.end_date);
    let third_party_signer_key = get_third_party_signer(guard_set.third_party_signer);
    let (mint_limit_id, mint_limit_limit) = get_mint_limit(guard_set.mint_limit);
    let (gatekeeper_expire_on_use, gatekeeper_network) =
        get_gatekeeper(guard_set.gatekeeper);
    let (freeze_sol_payment_lamports, freeze_sol_payment_destination) =
        get_freeze_sol_payment(guard_set.freeze_sol_payment);
    let (token_gate_amount, token_gate_mint) = get_token_gate(guard_set.token_gate);
    let nft_gate_required_collection = get_nft_gate(guard_set.nft_gate);
    let (token_burn_amount, token_burn_mint) = get_token_burn(guard_set.token_burn);
    let nft_burn_required_collection = get_nft_burn(guard_set.nft_burn);
    let (token_payment_amount, token_payment_mint, token_payment_destination_ata) =
        get_token_payment(guard_set.token_payment);
    let allow_list_merkle_root = get_allow_list(guard_set.allow_list);

    DBGuardSet {
        bot_tax_lamports,
        bot_tax_last_instruction,
        start_date,
        end_date,
        third_party_signer_key,
        nft_payment_destination,
        nft_payment_required_collection,
        mint_limit_id,
        mint_limit_limit,
        gatekeeper_network,
        gatekeeper_expire_on_use,
        sol_payment_lamports,
        sol_payment_destination,
        redeemed_amount_maximum,
        address_gate_address,
        freeze_sol_payment_lamports,
        freeze_sol_payment_destination,
        token_gate_amount,
        token_gate_mint,
        nft_gate_required_collection,
        token_burn_amount,
        token_burn_mint,
        nft_burn_required_collection,
        token_payment_amount,
        token_payment_mint,
        token_payment_destination_ata,
        allow_list_merkle_root,
        freeze_token_payment_amount,
        freeze_token_payment_mint,
        freeze_token_payment_destination_ata,
    }
}
