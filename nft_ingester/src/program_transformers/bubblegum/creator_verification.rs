use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use digital_asset_types::dao::{asset, asset_creators};
use sea_orm::{ConnectionTrait, Set, TransactionTrait, Unchanged};

use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{update_asset, update_creator},
};

use super::save_changelog_event;

pub async fn process<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    value: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let maybe_creator = match parsing_result.payload {
        Some(Payload::VerifyCreator { creator }) => Some(creator),
        Some(Payload::UnverifyCreator { creator }) => Some(creator),
        _ => None,
    };

    if let (Some(le), Some(cl), Some(creator)) = (
        &parsing_result.leaf_update,
        &parsing_result.tree_update,
        maybe_creator,
    ) {
        // Do we need to update the `slot_updated` field as well as part of the table
        // updates below?

        let seq = save_changelog_event(cl, bundle.slot, txn).await?;
        let asset_id_bytes = match le.schema {
            LeafSchema::V1 { id, .. } => {
                let id_bytes = id.to_bytes().to_vec();
                let asset_to_update = asset::ActiveModel {
                    id: Unchanged(id_bytes.clone()),
                    leaf: Set(Some(le.leaf_hash.to_vec())),
                    seq: Set(seq as i64),
                    ..Default::default()
                };

                update_asset(txn, id_bytes.clone(), Some(seq), asset_to_update).await?;
                id_bytes
            } // _ => return Err(IngesterError::NotImplemented),
        };

        // The primary key `id` is not required here since `update_creator` uses `update_many`
        // for the time being.
        let creator_to_update = asset_creators::ActiveModel {
            //id: Unchanged(14),
            verified: Set(value),
            seq: Set(seq as i64),
            ..Default::default()
        };

        update_creator(
            txn,
            asset_id_bytes,
            creator.to_bytes().to_vec(),
            seq,
            creator_to_update,
        )
        .await?;

        return Ok(());
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
