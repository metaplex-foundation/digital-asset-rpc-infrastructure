use std::future::Future;
use std::pin::Pin;
use sea_orm::{entity::*, query::*, EntityTrait, ColumnTrait, DbErr, DatabaseTransaction, ConnectionTrait, DbBackend};
use blockbuster::instruction::InstructionBundle;
use blockbuster::programs::bubblegum::{BubblegumInstruction, Payload};
use digital_asset_types::dao::asset;
use crate::IngesterError;

pub async fn decompress<'c>(parsing_result: &BubblegumInstruction, bundle: &InstructionBundle<'c>, txn: &'c DatabaseTransaction) -> Result<(), IngesterError> {
    let id_bytes = bundle.keys.get(3).unwrap().0.as_slice().to_vec();

    let model = asset::ActiveModel {
        id: Unchanged(id_bytes.clone()),
        leaf: Set(None),
        compressed: Set(false),
        compressible: Set(false),
        supply: Set(1),
        supply_mint: Set(Some(id_bytes.clone())),
        ..Default::default()
    };

    // After the decompress instruction runs, the asset is no longer managed
    // by Bubblegum and Gummyroll, so there will not be any other instructions
    // after this one.
    //
    // Do not run this command if the asset is already marked as
    // decompressed.
    let query = asset::Entity::update(model)
        .filter(asset::Column::Id.eq(id_bytes.clone()))
        .filter(asset::Column::Compressed.eq(true))
        .build(DbBackend::Postgres);

    txn.execute(query).await.map(|_| ()).map_err(Into::into)
}
