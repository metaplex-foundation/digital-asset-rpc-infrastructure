use crate::{
    program_transformers::{bubblegum::db::update_asset, common::save_changelog_event},
    IngesterError,
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema},
};
use digital_asset_types::dao::generated::asset;
use sea_orm::{entity::*, DatabaseTransaction};

pub async fn burn<'c>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c DatabaseTransaction,
) -> Result<(), IngesterError> {
    if let (Some(le), Some(cl)) = (&parsing_result.leaf_update, &parsing_result.tree_update) {
        let seq = save_changelog_event(&cl, bundle.slot, txn).await?;
        return match le.schema {
            LeafSchema::V1 { id, .. } => {
                let id_bytes = id.to_bytes().to_vec();
                let asset_to_update = asset::ActiveModel {
                    id: Unchanged(id_bytes.clone()),
                    burnt: Set(true),
                    seq: Set(seq as i64), // gummyroll seq
                    ..Default::default()
                };
                // Don't send sequence number with this update, because we will always
                // run this update even if it's from a backfill/replay.
                update_asset(txn, id_bytes, None, asset_to_update).await
            }
            _ => Err(IngesterError::NotImplemented),
        };
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
