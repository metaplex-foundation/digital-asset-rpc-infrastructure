use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        save_changelog_event, upsert_asset_with_leaf_info,
        upsert_asset_with_owner_and_delegate_info, upsert_asset_with_seq, upsert_creator_verified,
    },
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use log::debug;
use sea_orm::{ConnectionTrait, TransactionTrait};

pub async fn process<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    value: bool,
    instruction: &str,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (Some(le), Some(cl), Some(payload)) = (
        &parsing_result.leaf_update,
        &parsing_result.tree_update,
        &parsing_result.payload,
    ) {
        let (creator, verify) = match payload {
            Payload::CreatorVerification {
                creator, verify, ..
            } => (creator, verify),
            _ => {
                return Err(IngesterError::ParsingError(
                    "Ix not parsed correctly".to_string(),
                ));
            }
        };
        debug!(
            "Handling creator verification event for creator {} (verify: {}): {}",
            creator, verify, bundle.txn_id
        );
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, instruction).await?;

        let asset_id_bytes = match le.schema {
            LeafSchema::V1 {
                id,
                owner,
                delegate,
                ..
            } => {
                let id_bytes = id.to_bytes();
                let owner_bytes = owner.to_bytes().to_vec();
                let delegate = if owner == delegate || delegate.to_bytes() == [0; 32] {
                    None
                } else {
                    Some(delegate.to_bytes().to_vec())
                };
                let tree_id = cl.id.to_bytes();
                let nonce = cl.index as i64;

                // Partial update of asset table with just leaf info.
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

                // Partial update of asset table with just leaf owner and delegate.
                upsert_asset_with_owner_and_delegate_info(
                    txn,
                    id_bytes.to_vec(),
                    owner_bytes,
                    delegate,
                    seq as i64,
                )
                .await?;

                upsert_asset_with_seq(txn, id_bytes.to_vec(), seq as i64).await?;

                id_bytes.to_vec()
            }
        };

        upsert_creator_verified(
            txn,
            asset_id_bytes,
            creator.to_bytes().to_vec(),
            value,
            seq as i64,
        )
        .await?;

        return Ok(());
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
