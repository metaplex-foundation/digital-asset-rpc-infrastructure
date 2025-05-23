use {
    crate::{
        bubblegum::db::{
            save_changelog_event, upsert_asset_creators, upsert_asset_with_leaf_info,
            upsert_asset_with_owner_and_delegate_info, upsert_asset_with_seq,
        },
        bubblegum::NormalizedLeafFields,
        error::{ProgramTransformerError, ProgramTransformerResult},
    },
    blockbuster::{
        instruction::InstructionBundle,
        programs::bubblegum::{BubblegumInstruction, Payload},
    },
    mpl_bubblegum::types::Creator,
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
        let (updated_creators, creator, verify) = match payload {
            Payload::CreatorVerification {
                metadata,
                creator,
                verify,
            } => {
                let updated_creators: Vec<Creator> = metadata
                    .creators
                    .iter()
                    .map(|c| {
                        let mut c = c.clone();
                        if c.address == *creator {
                            c.verified = *verify
                        };
                        c
                    })
                    .collect();

                (updated_creators, creator, verify)
            }
            _ => {
                return Err(ProgramTransformerError::ParsingError(
                    "Ix not parsed correctly".to_string(),
                ));
            }
        };
        debug!(
            "Handling creator verification event for creator {} (verify: {}): {}",
            creator, verify, bundle.txn_id
        );
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, instruction).await?;

        let leaf = NormalizedLeafFields::from(&le.schema);

        let id_bytes = leaf.id.to_bytes();
        let owner_bytes = leaf.owner.to_bytes().to_vec();
        let delegate = if leaf.owner == leaf.delegate || leaf.delegate.to_bytes() == [0; 32] {
            None
        } else {
            Some(leaf.delegate.to_bytes().to_vec())
        };
        let tree_id = cl.id.to_bytes();
        let nonce = cl.index as i64;

        // Begin a transaction.  If the transaction goes out of scope (i.e. one of the executions has
        // an error and this function returns it using the `?` operator), then the transaction is
        // automatically rolled back.
        let multi_txn = txn.begin().await?;

        // Partial update of asset table with just leaf info.
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

        // Upsert creators to `asset_creators` table.
        upsert_asset_creators(
            &multi_txn,
            id_bytes.to_vec(),
            &updated_creators,
            bundle.slot as i64,
            seq as i64,
        )
        .await?;

        multi_txn.commit().await?;

        return Ok(());
    }
    Err(ProgramTransformerError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
