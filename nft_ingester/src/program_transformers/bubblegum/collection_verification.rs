use blockbuster::instruction::InstructionBundle;
use blockbuster::programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload};
use digital_asset_types::dao::asset;
use sea_orm::{DatabaseTransaction, Set, Unchanged};

use crate::program_transformers::bubblegum::update_asset;
use crate::tasks::common::save_changelog_event;
use crate::IngesterError;

pub async fn process<'c>(
    parsing_result: &'c BubblegumInstruction,
    bundle: &'c InstructionBundle<'c>,
    txn: &'c DatabaseTransaction,
    verify: bool,
) -> Result<(), IngesterError> {
    if let (Some(le), Some(cl)) = (&parsing_result.leaf_update, &parsing_result.tree_update) {
        // Do we need to update the `slot_updated` field as well as part of the table
        // updates below?
        let seq = save_changelog_event(cl, bundle.slot, txn).await?;
        match le.schema {
            LeafSchema::V1 { id, .. } => {
                let id_bytes = id.to_bytes().to_vec();

                let mut asset_to_update = asset::ActiveModel {
                    id: Unchanged(id_bytes.clone()),
                    leaf: Set(Some(le.leaf_hash.to_vec())),
                    seq: Set(seq as i64),
                    // We can just set the value here instead of checking whether a collection
                    // is actually associated with the asset first, because the Bubblegum
                    // operations will not succeed if a verify operation is applied to an asset
                    // that doesn't belong to any collection.
                    collection_verified: Set(verify),
                    ..Default::default()
                };

                if let Some(Payload::SetAndVerifyCollection { collection }) = parsing_result.payload
                {
                    let collection_bytes = collection.to_bytes().to_vec();
                    asset_to_update.collection = Set(Some(collection_bytes));
                }

                update_asset(txn, id_bytes.clone(), Some(seq), asset_to_update).await?;
                id_bytes
            }
            _ => return Err(IngesterError::NotImplemented),
        };

        return Ok(());
    };
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
