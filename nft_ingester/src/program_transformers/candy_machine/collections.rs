use crate::IngesterError;

use digital_asset_types::dao::generated::candy_machine;

use blockbuster::programs::candy_machine::state::CollectionPDA;
use plerkle_serialization::Pubkey as FBPubkey;
use sea_orm::{
    entity::*, query::*, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, EntityTrait,
};

pub async fn collections(
    collections: &CollectionPDA,
    id: FBPubkey,
    txn: &DatabaseTransaction,
) -> Result<(), IngesterError> {
    let model = candy_machine::ActiveModel {
        id: Unchanged(collections.candy_machine.to_bytes().to_vec()),
        collection_mint: Set(Some(collections.mint.to_bytes().to_vec())),
        ..Default::default()
    };

    let query = candy_machine::Entity::update(model)
        .filter(candy_machine::Column::Id.eq(collections.candy_machine.to_bytes().to_vec()))
        .build(DbBackend::Postgres);

    txn.execute(query)
        .await
        .map(|_| ())
        .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;

    Ok(())
}
