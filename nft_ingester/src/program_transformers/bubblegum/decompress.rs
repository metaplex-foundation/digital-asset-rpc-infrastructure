use std::future::Future;
use std::pin::Pin;
use std::process::Output;
use sea_orm::ActiveValue::{Set, Unchanged};
use sea_orm::{ConnectionTrait, DatabaseTransaction, DbBackend};
use blockbuster::instruction::InstructionBundle;
use blockbuster::programs::bubblegum::{BubblegumInstruction, Payload};
use digital_asset_types::dao::asset;
use crate::IngesterError;
use crate::program_transformers::bubblegum::update_asset;
use crate::program_transformers::common::save_changelog_event;

pub fn decompress<'c>(parsing_result: &BubblegumInstruction, bundle: &InstructionBundle, txn: &DatabaseTransaction) -> Pin<Box<dyn Future<Output=Result<_, IngesterError>> + Send + 'c>> {
    Box::pin(async move {
        let id_bytes = bundle.keys[3].0;

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
            .filter(asset::Column::Id.eq(id_bytes))
            .filter(asset::Column::Compressed.eq(true))
            .build(DbBackend::Postgres);

        txn.execute(query).await.map(|_| ()).map_err(Into::into)
    })
}
