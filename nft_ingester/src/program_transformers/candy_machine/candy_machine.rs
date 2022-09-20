use crate::{
    program_transformers::{candy_machine::state::CandyMachine, common::save_changelog_event},
    IngesterError,
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use candy_machine::state::CandyMachine;
use digital_asset_types::{
    adapter::{TokenStandard, UseMethod, Uses},
    dao::{
        candy_machine_data, candy_machine_state,
        sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
    },
    json::ChainDataV1,
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

pub async fn candy_machine<'c>(
    candy_machine: &CandyMachine,
    acct: &AccountInfo<'c>,
    txn: &'c DatabaseTransaction,
) -> Result<(), IngesterError> {
    let data = candy_machine.data;

    let candy_machine_data = candy_machine_data::ActiveModel {
        uuid: Set(data.uuid),
        price: Set(data.price),
        symbol: Set(data.symbol),
        seller_fee_basis_points: Set(data.seller_fee_basis_points),
        max_suppy: Set(data.max_supply),
        is_mutable: Set(data.is_mutable),
        retain_authority: Set(data.retain_authority),
        go_live_date: Set(data.go_live_date),
        items_available: Set(data.items_available),
        ..Default::default()
    };

    let candy_machine_state = candy_machine_state::ActiveModel {
        candy_machine_data_id: Set(candy_machine_data.id),
        authority: Set(candy_machine.authority.to_bytes().to_vec()),
        wallet: Set(candy_machine.wallet.to_bytes().to_vec()),
        token_mint: Set(candy_machine.token_mint.to_bytes().to_vec()),
        items_redeemed: Set(candy_machine.items_redeemed),
        ..Default::default()
    };

    

    Ok(())
}
