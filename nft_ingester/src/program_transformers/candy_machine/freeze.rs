use crate::IngesterError;

use digital_asset_types::dao::generated::candy_machine;
use plerkle_serialization::Pubkey as FBPubkey;
use sea_orm::{entity::*, query::*, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait, DbErr};

use blockbuster::programs::candy_machine::state::FreezePDA;

pub async fn freeze(
    freeze: &FreezePDA,
    id: FBPubkey,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let id_bytes = id.0.to_vec();
    let candy_machine_freeze = candy_machine::ActiveModel {
        id: Unchanged(id_bytes),
        allow_thaw: Set(Some(freeze.allow_thaw)),
        frozen_count: Set(Some(freeze.frozen_count)),
        mint_start: Set(freeze.mint_start),
        freeze_time: Set(Some(freeze.freeze_time)),
        freeze_fee: Set(Some(freeze.freeze_fee)),
        ..Default::default()
    };

    let query = candy_machine::Entity::update(candy_machine_freeze)
        .filter(candy_machine::Column::Id.eq(freeze.candy_machine.to_bytes().to_vec()))
        .build(DbBackend::Postgres);

    txn.execute(query)
        .await
        .map(|_| ())
        .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;

    Ok(())
}
