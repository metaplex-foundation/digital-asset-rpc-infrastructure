use crate::{
    program_transformers::{candy_machine::state::CandyMachine, common::save_changelog_event},
    IngesterError,
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use candy_machine::state::CandyMachine;
use digital_asset_types::dao::candy_machine;
use num_traits::FromPrimitive;
use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DatabaseTransaction, DbBackend,
    EntityTrait, JsonValue,
};

use super::state::CollectionPDA;

pub async fn collections<'c>(
    collections: &CollectionPDA,
    acct: &AccountInfo<'c>,
    txn: &'c DatabaseTransaction,
) -> Result<(), IngesterError> {
    let model = candy_machine::ActiveModel {
        id: Unchanged(collections.candy_machine.to_bytes().to_vec()),
        collection_mint: Set(Some(collections.mint.to_bytes().to_vec())),
        ..Default::default()
    };

    let query = candy_machine::Entity::update(model)
        .filter(candy_machine::Column::Id.eq(collections.candy_machine.to_bytes().to_vec()))
        .build(DbBackend::Postgres);

    txn.execute(query).await.map(|_| ()).map_err(Into::into);

    Ok(())
}
