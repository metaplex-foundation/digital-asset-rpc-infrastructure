use crate::program_transformers::bubblegum::{upsert_asset_with_seq, upsert_collection_info};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use log::debug;
use mpl_bubblegum::types::Collection;
use sea_orm::query::*;

use super::{save_changelog_event, upsert_asset_with_leaf_info};
use crate::error::IngesterError;
pub async fn process<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    cl_audits: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (Some(le), Some(cl), Some(payload)) = (
        &parsing_result.leaf_update,
        &parsing_result.tree_update,
        &parsing_result.payload,
    ) {
        let (collection, verify) = match payload {
            Payload::CollectionVerification {
                collection, verify, ..
            } => (collection.clone(), verify.clone()),
            _ => {
                return Err(IngesterError::ParsingError(
                    "Ix not parsed correctly".to_string(),
                ));
            }
        };
        debug!(
            "Handling collection verification event for {} (verify: {}): {}",
            collection, verify, bundle.txn_id
        );
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, cl_audits).await?;
        let id_bytes = match le.schema {
            LeafSchema::V1 { id, .. } => id.to_bytes().to_vec(),
        };
        let tree_id = cl.id.to_bytes();
        let nonce = cl.index as i64;

        // Partial update of asset table with just leaf.
        upsert_asset_with_leaf_info(
            txn,
            id_bytes.to_vec(),
            nonce,
            tree_id.to_vec(),
            le.leaf_hash.to_vec(),
            le.schema.data_hash(),
            le.schema.creator_hash(),
            seq as i64,
            false,
        )
        .await?;

        upsert_asset_with_seq(txn, id_bytes.to_vec(), seq as i64).await?;

        upsert_collection_info(
            txn,
            id_bytes.to_vec(),
            Some(Collection {
                key: collection.clone(),
                verified: verify,
            }),
            bundle.slot as i64,
            seq as i64,
        )
        .await?;

        return Ok(());
    };
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
