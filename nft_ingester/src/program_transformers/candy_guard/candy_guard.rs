use crate::{
    program_transformers::{
        candy_machine::{helpers::process_whitelist_change, state::CandyMachine},
        common::save_changelog_event,
    },
    IngesterError,
};

use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use candy_machine::state::CandyMachine;
use digital_asset_types::{
    adapter::{TokenStandard, UseMethod, Uses},
    dao::{candy_guard, candy_guard_group, sea_orm_active_enums::WhitelistMintMode},
    rpc::LiveDate,
};
use mpl_candy_guard::{
    guards::{AllowList, EndSettings, Gatekeeper, SplToken, ThirdPartySigner, Whitelist},
    state::{CandyGuard, CandyGuardData},
};
use num_traits::FromPrimitive;
use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DatabaseTransaction, DbBackend,
    EntityTrait, JsonValue,
};
use solana_sdk::lamports;

use super::helpers::*;

pub async fn candy_guard<'c>(
    candy_guard: &CandyGuard,
    candy_guard_data: &CandyGuardData,
    acct: &AccountInfo<'c>,
    txn: &'c DatabaseTransaction,
) -> Result<(), IngesterError> {
    let candy_guard = candy_guard::ActiveModel {
        id: Set(candy_guard.base.to_bytes().to_vec()),
        bump: Set(candy_guard.bump),
        authority: Set(candy_guard.authority.to_bytes().to_vec()),
    };

    // TODO need to get from DB for value cm and update the candy guard pda value
    let query = candy_guard::Entity::insert(candy_guard)
        .on_conflict(
            OnConflict::columns([candy_guard::Column::Id])
                .update_columns([candy_guard::Column::Bump, candy_guard::Column::Authority])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    let (end_setting_type, number) = get_end_settings(candy_guard_data.end_settings);
    let (mode, presale, whitelist_mint, discount_price) =
        get_whitelist_settings(candy_guard_data.whitelist);
    let (gatekeeper_network, expire_on_use) =
        get_gatekeeper_network(candy_guard_data.gatekeeper_network);
    let merkle_root = get_allow_list(candy_guard_data.allow_list);
    let (lamports, last_instruction) = get_bot_tax(candy_guard_data.bot_tax);
    let (amount, destination) = get_lamports(candy_guard_data.lamports);
    let (spl_token_amount, token_mint, destination_ata) = get_spl_token(candy_guard_data.spl_token);
    let live_date = get_live_date(candy_guard_data.live_date);
    let signer_key = get_third_party_signer(candy_guard_data.third_party_signer);
    let (mint_limit_limit, mint_limit_id) = get_mint_limit(candy_guard_data.mint_limit);
    let (nft_payment_burn, nft_payment_required_collection) =
        get_nft_payment(candy_guard_data.nft_payment);

    // TODO edit init sql for more descriptive naming fields
    let candy_guard_default_set = candy_guard_group::ActiveModel {
        label: Set(None),
        candy_guard_id: Set(candy_guard.base.to_bytes().to_vec()),
        mode: Set(mode),
        whitelist_mint: Set(whitelist_mint),
        presale: Set(presale),
        discount_price: Set(discount_price),
        gatekeeper_network: Set(gatekeeper_network),
        expire_on_use: Set(expire_on_use),
        number: Set(number),
        end_setting_type: Set(end_setting_type),
        merkle_root: Set(merkle_root),
        amount: Set(amount),
        destination: Set(destination),
        signer_key: Set(signer_key),
        mint_limit_id: Set(mint_limit_id),
        mint_limit_limit: Set(mint_limit_limit),
        nft_payment_burn: Set(nft_payment_burn),
        nft_payment_required_collection: Set(nft_payment_required_collection),
        lamports: Set(lamports),
        last_instruction: Set(last_instruction),
        live_date: Set(live_date),
        spl_token_amount: Set(spl_token_amount),
        token_mint: Set(token_mint),
        destination_ata: Set(destination_ata),
        ..Default::default()
    };

    let query = candy_guard_group::Entity::insert(candy_guard_default_set)
        .on_conflict(
            OnConflict::columns([candy_guard_group::Column::Id])
                .update_columns([
                    candy_guard_group::Column::CandyGuardId,
                    candy_guard_group::Column::Label,
                    candy_guard_group::Column::Mode,
                    candy_guard_group::Column::WhitelistMint,
                    candy_guard_group::Column::Presale,
                    candy_guard_group::Column::DiscountPrice,
                    candy_guard_group::Column::GatekeeperNetwork,
                    candy_guard_group::Column::ExpireOnUse,
                    candy_guard_group::Column::Number,
                    candy_guard_group::Column::EndSettingType,
                    candy_guard_group::Column::MerkleRoot,
                    candy_guard_group::Column::Amount,
                    candy_guard_group::Column::Destination,
                    candy_guard_group::Column::SignerKey,
                    candy_guard_group::Column::MintLimitId,
                    candy_guard_group::Column::MintLimitLimit,
                    candy_guard_group::Column::NftPaymentBurn,
                    candy_guard_group::Column::NftPaymentRequiredCollection,
                    candy_guard_group::Column::Lamports,
                    candy_guard_group::Column::LastInstruction,
                    candy_guard_group::Column::LiveDate,
                    candy_guard_group::Column::SplTokenAmount,
                    candy_guard_group::Column::TokenMint,
                    candy_guard_group::Column::DestinationAta,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    if let Some(groups) = candy_guard_data.groups {
        if groups.len() > 0 {
            for g in groups.iter() {
                let (end_setting_type, number) = get_end_settings(candy_guard_data.end_settings);
                let (mode, presale, whitelist_mint, discount_price) =
                    get_whitelist_settings(candy_guard_data.whitelist);
                let (gatekeeper_network, expire_on_use) =
                    get_gatekeeper_network(candy_guard_data.gatekeeper_network);
                let (merkle_root) = get_allow_list(candy_guard_data.allow_list);
                let (lamports, last_instruction) = get_bot_tax(candy_guard_data.bot_tax);
                let (amount, destination) = get_lamports(candy_guard_data.lamports);
                let (spl_token_amount, token_mint, destination_ata) =
                    get_spl_token(candy_guard_data.spl_token);
                let (live_date) = get_live_date(candy_guard_data.live_date);
                let signer_key = get_third_party_signer(candy_guard_data.third_party_signer);
                let (mint_limit_limit, mint_limit_id) = get_mint_limit(candy_guard_data.mint_limit);
                let (nft_payment_burn, nft_payment_required_collection) =
                    get_nft_payment(candy_guard_data.nft_payment);

                let candy_guard_default_set = candy_guard_group::ActiveModel {
                    label: Set(Some(g.label)),
                    candy_guard_id: Set(candy_guard.base.to_bytes().to_vec()),
                    mode: Set(mode),
                    whitelist_mint: Set(whitelist_mint),
                    presale: Set(presale),
                    discount_price: Set(discount_price),
                    gatekeeper_network: Set(gatekeeper_network),
                    expire_on_use: Set(expire_on_use),
                    number: Set(number),
                    end_setting_type: Set(end_setting_type),
                    merkle_root: Set(merkle_root),
                    amount: Set(amount),
                    destination: Set(destination),
                    signer_key: Set(signer_key),
                    mint_limit_id: Set(mint_limit_id),
                    mint_limit_limit: Set(mint_limit_limit),
                    nft_payment_burn: Set(nft_payment_burn),
                    nft_payment_required_collection: Set(nft_payment_required_collection),
                    lamports: Set(lamports),
                    last_instruction: Set(last_instruction),
                    live_date: Set(live_date),
                    spl_token_amount: Set(spl_token_amount),
                    token_mint: Set(token_mint),
                    destination_ata: Set(destination_ata),
                    ..Default::default()
                };

                let query = candy_guard_group::Entity::insert(candy_guard_group)
                    .on_conflict(
                        OnConflict::columns([candy_guard_group::Column::CandyGuardId])
                            .update_columns([
                                candy_guard_group::Column::CandyGuardId,
                                candy_guard_group::Column::Label,
                                candy_guard_group::Column::Mode,
                                candy_guard_group::Column::WhitelistMint,
                                candy_guard_group::Column::Presale,
                                candy_guard_group::Column::DiscountPrice,
                                candy_guard_group::Column::GatekeeperNetwork,
                                candy_guard_group::Column::ExpireOnUse,
                                candy_guard_group::Column::Number,
                                candy_guard_group::Column::EndSettingType,
                                candy_guard_group::Column::MerkleRoot,
                                candy_guard_group::Column::Amount,
                                candy_guard_group::Column::Destination,
                                candy_guard_group::Column::SignerKey,
                                candy_guard_group::Column::MintLimitId,
                                candy_guard_group::Column::MintLimitLimit,
                                candy_guard_group::Column::NftPaymentBurn,
                                candy_guard_group::Column::NftPaymentRequiredCollection,
                                candy_guard_group::Column::Lamports,
                                candy_guard_group::Column::LastInstruction,
                                candy_guard_group::Column::LiveDate,
                                candy_guard_group::Column::SplTokenAmount,
                                candy_guard_group::Column::TokenMint,
                                candy_guard_group::Column::DestinationAta,
                            ])
                            .to_owned(),
                    )
                    .build(DbBackend::Postgres);
                txn.execute(query).await.map(|_| ()).map_err(Into::into);
            }
        };
    }

    Ok(())
}
