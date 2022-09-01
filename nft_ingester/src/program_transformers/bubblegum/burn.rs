use std::future::Future;
use std::pin::Pin;
use std::process::Output;
use sea_orm::ActiveValue::{Set, Unchanged};
use sea_orm::DatabaseTransaction;
use blockbuster::instruction::InstructionBundle;
use blockbuster::programs::bubblegum::BubblegumInstruction;
use digital_asset_types::dao::asset;
use crate::IngesterError;
use crate::program_transformers::bubblegum::update_asset;
use crate::program_transformers::common::save_changelog_event;

pub fn burn<'c>(parsing_result: &BubblegumInstruction, bundle: &InstructionBundle, txn: &DatabaseTransaction) -> Pin<Box<dyn Future<Output=Result<_, IngesterError>> + Send + 'c>> {
    Box::pin(async move {
        if let Some(cl) = parsing_result.tree_update {
            save_changelog_event(&cl, bundle.slot, txn)
                .await?
                .ok_or(IngesterError::ChangeLogEventMalformed)?;
        }
        if let Some(le) = &parsing_result.leaf_update {
            match le.schema {
                LeafSchema::V1 { id, .. } => {
                    let id_bytes = id.to_bytes().to_vec();
                    let asset_to_update = asset::ActiveModel {
                        id: Unchanged(id_bytes.clone()),
                        burnt: Set(true),
                        ..Default::default()
                    };
// Don't send sequence number with this update, because we will always
// run this update even if it's from a backfill/replay.
                    update_asset(txn, id_bytes, None, asset_to_update).await
                }
                _ => Err(IngesterError::NotImplemented),
            }
        }
        Err(IngesterError::ParsingError("Ix not parsed correctly".to_string()))
    })
}
