use {
    crate::{
        bubblegum::{
            db::{
                save_changelog_event, upsert_asset_with_leaf_info, upsert_asset_with_seq,
                upsert_collection_info,
            },
            NormalizedLeafFields,
        },
        error::{ProgramTransformerError, ProgramTransformerResult},
    },
    blockbuster::{
        instruction::InstructionBundle,
        programs::bubblegum::{BubblegumInstruction, Payload, ID as MPL_BUBBLEGUM_ID},
    },
    mpl_bubblegum::types::Collection,
    sea_orm::{ConnectionTrait, TransactionTrait},
    tracing::debug,
};

pub async fn process<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    instruction: &str,
) -> ProgramTransformerResult<()>
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
            } => (collection, verify),
            _ => {
                return Err(ProgramTransformerError::ParsingError(
                    "Ix not parsed correctly".to_string(),
                ));
            }
        };
        debug!(
            "Handling collection verification event for {} (verify: {}): {}",
            collection, verify, bundle.txn_id
        );
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, instruction).await?;

        let leaf = NormalizedLeafFields::from(&le.schema);

        let id_bytes = leaf.id.to_bytes();
        let tree_id = cl.id.to_bytes();
        let nonce = cl.index as i64;

        // Begin a transaction.  If the transaction goes out of scope (i.e. one of the executions has
        // an error and this function returns it using the `?` operator), then the transaction is
        // automatically rolled back.
        let multi_txn = txn.begin().await?;

        // Partial update of asset table with just leaf.
        upsert_asset_with_leaf_info(
            &multi_txn,
            id_bytes.to_vec(),
            nonce,
            tree_id.to_vec(),
            le.leaf_hash.to_vec(),
            leaf.data_hash,
            leaf.creator_hash,
            leaf.collection_hash,
            leaf.asset_data_hash,
            leaf.flags,
            seq as i64,
        )
        .await?;

        upsert_asset_with_seq(&multi_txn, id_bytes.to_vec(), seq as i64).await?;

        // If the collection ID is the MPL Bubblegum program ID, it means the new MPL Core
        // collection was set to `Option::None` in the `SetCollectionV2` account validation
        // struct, and thus the asset as been removed from any collection.
        let collection = if collection == &MPL_BUBBLEGUM_ID {
            None
        } else {
            Some(Collection {
                key: *collection,
                verified: *verify,
            })
        };

        upsert_collection_info(
            &multi_txn,
            id_bytes.to_vec(),
            collection,
            bundle.slot as i64,
            seq as i64,
        )
        .await?;

        multi_txn.commit().await?;

        return Ok(());
    };
    Err(ProgramTransformerError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
