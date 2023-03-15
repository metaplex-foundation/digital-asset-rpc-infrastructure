use crate::{
    error::IngesterError,
};
use super::{update_asset, save_changelog_event};
use blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction};
use digital_asset_types::dao::asset;
use sea_orm::{
    entity::*, query::*, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
};

pub async fn decompress<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
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
