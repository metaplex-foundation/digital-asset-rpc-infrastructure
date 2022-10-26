use crate::IngesterError;

use digital_asset_types::dao::generated::{
    candy_guard, candy_guard_group, candy_machine, prelude::CandyMachine,
};

use mpl_candy_guard::state::{CandyGuard, CandyGuardData};
use plerkle_serialization::Pubkey as FBPubkey;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DatabaseConnection,
    DatabaseTransaction, DbBackend, DbErr, EntityTrait,
};

use super::helpers::*;

pub async fn candy_guard<'c>(
    candy_guard: &CandyGuard,
    candy_guard_data: CandyGuardData,
    id: FBPubkey,
    txn: &DatabaseTransaction,
    db: &DatabaseConnection,
) -> Result<(), IngesterError> {
    let id_bytes = id.0.to_vec();
    let candy_guard_model = candy_guard::ActiveModel {
        id: Set(id_bytes.clone()),
        base: Set(candy_guard.base.to_bytes().to_vec()),
        bump: Set(candy_guard.bump as i16),
        authority: Set(candy_guard.authority.to_bytes().to_vec()),
    };

    // this is returning a vec because candy guards can wrap multiple candy machines
    // but a single candy machine can have just one guard
    let candy_machines: Vec<candy_machine::Model> = CandyMachine::find()
        .filter(
            Condition::all()
                .add(candy_machine::Column::MintAuthority.is_not_null())
                .add(candy_machine::Column::MintAuthority.eq(id_bytes.clone()))
                .add(candy_machine::Column::CandyGuardId.is_null()),
        )
        .all(db)
        .await
        .and_then(|o| match o {
            o => Ok(o),
            _ => Err(DbErr::RecordNotFound(
                "Candy Machines Not Found".to_string(),
            )),
        })?;

    if candy_machines.len() > 0 {
        for cm in candy_machines.iter() {
            let candy_machine_model = candy_machine::ActiveModel {
                candy_guard_id: Set(Some(id_bytes.clone())),
                ..Default::default()
            };

            let query = candy_machine::Entity::insert(candy_machine_model)
                .on_conflict(
                    OnConflict::columns([candy_machine::Column::Id])
                        .update_columns([candy_machine::Column::CandyGuardId])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);

            txn.execute(query)
                .await
                .map(|_| ())
                .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;
        }
    }

    // TODO question: should we look for candy machines that have been wrapped and then unwrapped
    // do a check to see if candy guard is present and if so that it matches current mint authority
    // if they are different we can say that cm has been unwrapped by guard and set candy_guard_id back to null
    // changes done in candy machine file
    let query = candy_guard::Entity::insert(candy_guard_model)
        .on_conflict(
            OnConflict::columns([candy_guard::Column::Id])
                .update_columns([candy_guard::Column::Authority])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    txn.execute(query)
        .await
        .map(|_| ())
        .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;

    let default_guard = get_all_guards(&candy_guard_data.clone().default);

    let candy_guard_default_set = candy_guard_group::ActiveModel {
        label: Set(None),
        candy_guard_id: Set(id_bytes.clone()),
        bot_tax_lamports: Set(default_guard.bot_tax_lamports),
        bot_tax_last_instruction: Set(default_guard.bot_tax_last_instruction),
        start_date: Set(default_guard.start_date),
        end_date: Set(default_guard.end_date),
        third_party_signer_key: Set(default_guard.third_party_signer_key),
        nft_payment_destination: Set(default_guard.nft_payment_destination),
        nft_payment_required_collection: Set(default_guard.nft_payment_required_collection),
        mint_limit_id: Set(default_guard.mint_limit_id),
        mint_limit_limit: Set(default_guard.mint_limit_limit),
        gatekeeper_network: Set(default_guard.gatekeeper_network),
        gatekeeper_expire_on_use: Set(default_guard.gatekeeper_expire_on_use),
        sol_payment_lamports: Set(default_guard.sol_payment_lamports),
        sol_payment_destination: Set(default_guard.sol_payment_destination),
        redeemed_amount_maximum: Set(default_guard.redeemed_amount_maximum),
        address_gate_address: Set(default_guard.address_gate_address),
        freeze_sol_payment_lamports: Set(default_guard.freeze_sol_payment_lamports),
        freeze_sol_payment_destination: Set(default_guard.freeze_sol_payment_destination),
        nft_gate_required_collection: Set(default_guard.nft_gate_required_collection),
        token_burn_amount: Set(default_guard.token_burn_amount),
        token_burn_mint: Set(default_guard.token_burn_mint),
        nft_burn_required_collection: Set(default_guard.nft_burn_required_collection),
        token_payment_amount: Set(default_guard.token_payment_amount),
        token_payment_mint: Set(default_guard.token_payment_mint),
        token_payment_destination_ata: Set(default_guard.token_payment_destination_ata),
        allow_list_merkle_root: Set(default_guard.allow_list_merkle_root),
        freeze_token_payment_amount: Set(default_guard.freeze_token_payment_amount),
        freeze_token_payment_mint: Set(default_guard.freeze_token_payment_mint),
        freeze_token_payment_destination_ata: Set(
            default_guard.freeze_token_payment_destination_ata
        ),
        ..Default::default()
    };

    let query = candy_guard_group::Entity::insert(candy_guard_default_set)
        .on_conflict(
            OnConflict::columns([candy_guard_group::Column::Id])
                .update_columns([
                    candy_guard_group::Column::Label,
                    candy_guard_group::Column::GatekeeperNetwork,
                    candy_guard_group::Column::GatekeeperExpireOnUse,
                    candy_guard_group::Column::AllowListMerkleRoot,
                    candy_guard_group::Column::ThirdPartySignerKey,
                    candy_guard_group::Column::MintLimitId,
                    candy_guard_group::Column::MintLimitLimit,
                    candy_guard_group::Column::NftPaymentDestination,
                    candy_guard_group::Column::NftPaymentRequiredCollection,
                    candy_guard_group::Column::BotTaxLamports,
                    candy_guard_group::Column::BotTaxLastInstruction,
                    candy_guard_group::Column::SolPaymentDestination,
                    candy_guard_group::Column::SolPaymentLamports,
                    candy_guard_group::Column::StartDate,
                    candy_guard_group::Column::EndDate,
                    candy_guard_group::Column::RedeemedAmountMaximum,
                    candy_guard_group::Column::AddressGateAddress,
                    candy_guard_group::Column::FreezeSolPaymentLamports,
                    candy_guard_group::Column::FreezeSolPaymentDestination,
                    candy_guard_group::Column::TokenGateAmount,
                    candy_guard_group::Column::TokenGateMint,
                    candy_guard_group::Column::NftGateRequiredCollection,
                    candy_guard_group::Column::TokenBurnAmount,
                    candy_guard_group::Column::TokenBurnMint,
                    candy_guard_group::Column::NftBurnRequiredCollection,
                    candy_guard_group::Column::TokenPaymentAmount,
                    candy_guard_group::Column::TokenPaymentDestinationAta,
                    candy_guard_group::Column::TokenPaymentMint,
                    candy_guard_group::Column::FreezeTokenPaymentAmount,
                    candy_guard_group::Column::FreezeSolPaymentDestination,
                    candy_guard_group::Column::FreezeSolPaymentLamports,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);

    txn.execute(query)
        .await
        .map(|_| ())
        .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;

    if let Some(groups) = candy_guard_data.clone().groups {
        if groups.len() > 0 {
            for g in groups.iter() {
                let guards = get_all_guards(&g.clone().guards);

                let candy_guard_group = candy_guard_group::ActiveModel {
                    label: Set(Some(g.clone().label)),
                    candy_guard_id: Set(id_bytes.clone()),
                    bot_tax_lamports: Set(guards.bot_tax_lamports),
                    bot_tax_last_instruction: Set(guards.bot_tax_last_instruction),
                    start_date: Set(guards.start_date),
                    end_date: Set(guards.end_date),
                    third_party_signer_key: Set(guards.third_party_signer_key),
                    nft_payment_destination: Set(guards.nft_payment_destination),
                    nft_payment_required_collection: Set(guards.nft_payment_required_collection),
                    mint_limit_id: Set(guards.mint_limit_id),
                    mint_limit_limit: Set(guards.mint_limit_limit),
                    gatekeeper_network: Set(guards.gatekeeper_network),
                    gatekeeper_expire_on_use: Set(guards.gatekeeper_expire_on_use),
                    sol_payment_lamports: Set(guards.sol_payment_lamports),
                    sol_payment_destination: Set(guards.sol_payment_destination),
                    redeemed_amount_maximum: Set(guards.redeemed_amount_maximum),
                    address_gate_address: Set(guards.address_gate_address),
                    freeze_sol_payment_lamports: Set(guards.freeze_sol_payment_lamports),
                    freeze_sol_payment_destination: Set(guards.freeze_sol_payment_destination),
                    nft_gate_required_collection: Set(guards.nft_gate_required_collection),
                    token_burn_amount: Set(guards.token_burn_amount),
                    token_burn_mint: Set(guards.token_burn_mint),
                    nft_burn_required_collection: Set(guards.nft_burn_required_collection),
                    token_payment_amount: Set(guards.token_payment_amount),
                    token_payment_mint: Set(guards.token_payment_mint),
                    token_payment_destination_ata: Set(guards.token_payment_destination_ata),
                    allow_list_merkle_root: Set(guards.allow_list_merkle_root),
                    freeze_token_payment_amount: Set(guards.freeze_token_payment_amount),
                    freeze_token_payment_mint: Set(guards.freeze_token_payment_mint),
                    freeze_token_payment_destination_ata: Set(
                        guards.freeze_token_payment_destination_ata
                    ),
                    ..Default::default()
                };

                let query = candy_guard_group::Entity::insert(candy_guard_group)
                    .on_conflict(
                        OnConflict::columns([candy_guard_group::Column::CandyGuardId])
                            .update_columns([
                                candy_guard_group::Column::Label,
                                candy_guard_group::Column::GatekeeperNetwork,
                                candy_guard_group::Column::GatekeeperExpireOnUse,
                                candy_guard_group::Column::AllowListMerkleRoot,
                                candy_guard_group::Column::ThirdPartySignerKey,
                                candy_guard_group::Column::MintLimitId,
                                candy_guard_group::Column::MintLimitLimit,
                                candy_guard_group::Column::NftPaymentDestination,
                                candy_guard_group::Column::NftPaymentRequiredCollection,
                                candy_guard_group::Column::BotTaxLamports,
                                candy_guard_group::Column::BotTaxLastInstruction,
                                candy_guard_group::Column::SolPaymentDestination,
                                candy_guard_group::Column::SolPaymentLamports,
                                candy_guard_group::Column::StartDate,
                                candy_guard_group::Column::EndDate,
                                candy_guard_group::Column::RedeemedAmountMaximum,
                                candy_guard_group::Column::AddressGateAddress,
                                candy_guard_group::Column::FreezeSolPaymentLamports,
                                candy_guard_group::Column::FreezeSolPaymentDestination,
                                candy_guard_group::Column::TokenGateAmount,
                                candy_guard_group::Column::TokenGateMint,
                                candy_guard_group::Column::NftGateRequiredCollection,
                                candy_guard_group::Column::TokenBurnAmount,
                                candy_guard_group::Column::TokenBurnMint,
                                candy_guard_group::Column::NftBurnRequiredCollection,
                                candy_guard_group::Column::TokenPaymentAmount,
                                candy_guard_group::Column::TokenPaymentDestinationAta,
                                candy_guard_group::Column::TokenPaymentMint,
                                candy_guard_group::Column::FreezeTokenPaymentAmount,
                                candy_guard_group::Column::FreezeSolPaymentDestination,
                                candy_guard_group::Column::FreezeSolPaymentLamports,
                            ])
                            .to_owned(),
                    )
                    .build(DbBackend::Postgres);

                txn.execute(query)
                    .await
                    .map(|_| ())
                    .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;
            }
        };
    }

    Ok(())
}
