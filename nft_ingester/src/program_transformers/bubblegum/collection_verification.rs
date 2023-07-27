use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        save_changelog_event, upsert_asset_with_leaf_info,
        upsert_asset_with_owner_and_delegate_info, upsert_asset_with_seq, upsert_collection_info,
        upsert_collection_verified,
    },
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use sea_orm::query::*;

pub async fn process<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    verify: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (Some(le), Some(cl)) = (&parsing_result.leaf_update, &parsing_result.tree_update) {
        // Do we need to update the `slot_updated` field as well as part of the table
        // updates below?
        let seq = save_changelog_event(cl, bundle.slot, txn).await?;
        #[allow(unreachable_patterns)]
        match le.schema {
            LeafSchema::V1 {
                id,
                owner,
                delegate,
                ..
            } => {
                let id_bytes = id.to_bytes();
                let owner_bytes = owner.to_bytes().to_vec();
                let delegate = if owner == delegate {
                    None
                } else {
                    Some(delegate.to_bytes().to_vec())
                };

                // Partial update of asset table with just leaf.
                upsert_asset_with_leaf_info(
                    txn,
                    id_bytes.to_vec(),
                    Some(le.leaf_hash.to_vec()),
                    Some(seq as i64),
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

                if let Some(Payload::SetAndVerifyCollection { collection }) = parsing_result.payload
                {
                    // Upsert into `asset_grouping` table with base collection info.
                    upsert_collection_info(
                        txn,
                        id_bytes.to_vec(),
                        collection.to_string(),
                        bundle.slot as i64,
                        seq as i64,
                    )
                    .await?;
                }

                // Partial update with whether collection is verified and the `seq` number.
                upsert_collection_verified(txn, id_bytes.to_vec(), verify, seq as i64).await?;

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
