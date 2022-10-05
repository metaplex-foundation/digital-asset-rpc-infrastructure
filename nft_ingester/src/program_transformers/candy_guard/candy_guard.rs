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
    candy_guard_data: &CandyGuardData,
    id: FBPubkey,
    txn: &DatabaseTransaction,
    db: &DatabaseConnection,
) -> Result<(), IngesterError> {
    let id_bytes = id.0.to_vec();
    let candy_guard_model = candy_guard::ActiveModel {
        id: Set(candy_guard.base.to_bytes().to_vec()),
        base: Set(candy_guard.base.to_bytes().to_vec()),
        bump: Set(candy_guard.bump),
        authority: Set(candy_guard.authority.to_bytes().to_vec()),
    };

    // TODO need to get from DB for value cm and update the candy guard pda value
    // i think that the candy_guard acc.key should be primary key and update any CMs that now have mint authority as a candy guard
    let candy_machine: candy_machine::Model = CandyMachine::find_by_id(id_bytes)
        .one(db)
        .await
        .and_then(|o| match o {
            Some(a) => Ok(a),
            _ => Err(DbErr::RecordNotFound("Candy Machine Not Found".to_string())),
        })?;

    let query = candy_guard::Entity::insert(candy_guard_model)
        .on_conflict(
            OnConflict::columns([candy_guard::Column::Id])
                .update_columns([candy_guard::Column::Bump, candy_guard::Column::Authority])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query)
        .await
        .map(|_| ())
        .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;

    let (expire_on_use, gatekeeper_network) =
        get_gatekeeper(candy_guard_data.clone().default.gatekeeper);
    let merkle_root = get_allow_list(candy_guard_data.clone().default.allow_list);
    let (lamports, last_instruction) = get_bot_tax(candy_guard_data.clone().default.bot_tax);
    let signer_key = get_third_party_signer(candy_guard_data.clone().default.third_party_signer);
    let (mint_limit_id, mint_limit_limit) =
        get_mint_limit(candy_guard_data.clone().default.mint_limit);
    let (nft_payment_destination, nft_payment_required_collection) =
        get_nft_payment(candy_guard_data.clone().default.nft_payment);

    // TODO remove removed items from guard in init sql and entity files
    let candy_guard_default_set = candy_guard_group::ActiveModel {
        label: Set(None),
        candy_guard_id: Set(candy_guard.base.to_bytes().to_vec()),
        gatekeeper_network: Set(gatekeeper_network),
        gatekeeper_expire_on_use: Set(expire_on_use),
        allow_list_merkle_root: Set(merkle_root),
        third_party_signer_key: Set(signer_key),
        mint_limit_id: Set(mint_limit_id),
        mint_limit_limit: Set(mint_limit_limit),
        nft_payment_destination: Set(nft_payment_destination),
        nft_payment_required_collection: Set(nft_payment_required_collection),
        bot_tax_lamports: Set(lamports),
        bot_tax_last_instruction: Set(last_instruction),
        ..Default::default()
    };

    let query = candy_guard_group::Entity::insert(candy_guard_default_set)
        .on_conflict(
            OnConflict::columns([candy_guard_group::Column::Id])
                .update_columns([
                    candy_guard_group::Column::CandyGuardId,
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
                let (expire_on_use, gatekeeper_network) =
                    get_gatekeeper(g.clone().guards.gatekeeper);
                let merkle_root = get_allow_list(g.clone().guards.allow_list);
                let (lamports, last_instruction) = get_bot_tax(g.clone().guards.bot_tax);
                let signer_key = get_third_party_signer(g.clone().guards.third_party_signer);
                let (mint_limit_id, mint_limit_limit) = get_mint_limit(g.clone().guards.mint_limit);
                let (nft_payment_destination, nft_payment_required_collection) =
                    get_nft_payment(g.clone().guards.nft_payment);

                let candy_guard_group = candy_guard_group::ActiveModel {
                    label: Set(Some(g.clone().label)),
                    // TODO change candy guard id below
                    candy_guard_id: Set(candy_guard.base.to_bytes().to_vec()),
                    gatekeeper_network: Set(gatekeeper_network),
                    gatekeeper_expire_on_use: Set(expire_on_use),
                    allow_list_merkle_root: Set(merkle_root),
                    third_party_signer_key: Set(signer_key),
                    mint_limit_id: Set(mint_limit_id),
                    mint_limit_limit: Set(mint_limit_limit),
                    nft_payment_destination: Set(nft_payment_destination),
                    nft_payment_required_collection: Set(nft_payment_required_collection),
                    bot_tax_lamports: Set(lamports),
                    bot_tax_last_instruction: Set(last_instruction),
                    ..Default::default()
                };

                let query = candy_guard_group::Entity::insert(candy_guard_group)
                    .on_conflict(
                        OnConflict::columns([candy_guard_group::Column::CandyGuardId])
                            .update_columns([
                                candy_guard_group::Column::CandyGuardId,
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
