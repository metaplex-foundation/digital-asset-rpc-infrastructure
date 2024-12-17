use {
    crate::{
        bubblegum::db::{
            save_changelog_event, upsert_asset_with_leaf_info,
            upsert_asset_with_owner_and_delegate_info, upsert_asset_with_seq,
        },
        error::{ProgramTransformerError, ProgramTransformerResult},
    },
    blockbuster::{
        instruction::InstructionBundle,
        programs::bubblegum::{BubblegumInstruction, LeafSchema},
    },
    sea_orm::{ConnectionTrait, TransactionTrait},
};

pub async fn delegate<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    instruction: &str,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (Some(le), Some(cl)) = (&parsing_result.leaf_update, &parsing_result.tree_update) {
        // Begin a transaction.  If the transaction goes out of scope (i.e. one of the executions has
        // an error and this function returns it using the `?` operator), then the transaction is
        // automatically rolled back.
        let multi_txn = txn.begin().await?;

        let seq =
            save_changelog_event(cl, bundle.slot, bundle.txn_id, &multi_txn, instruction).await?;

        match le.schema {
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

                // Partial update of asset table with just leaf.
                upsert_asset_with_leaf_info(
                    &multi_txn,
                    id_bytes.to_vec(),
                    cl.index as i64,
                    tree_id.to_vec(),
                    le.leaf_hash.to_vec(),
                    le.schema.data_hash(),
                    le.schema.creator_hash(),
                    seq as i64,
                )
                .await?;

                // Partial update of asset table with just leaf owner and delegate.
                upsert_asset_with_owner_and_delegate_info(
                    &multi_txn,
                    id_bytes.to_vec(),
                    owner_bytes,
                    delegate,
                    seq as i64,
                )
                .await?;

                upsert_asset_with_seq(&multi_txn, id_bytes.to_vec(), seq as i64).await?;

                multi_txn.commit().await?;

                return Ok(());
            }
        }
    }
    Err(ProgramTransformerError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
