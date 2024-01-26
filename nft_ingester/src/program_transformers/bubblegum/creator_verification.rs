use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        save_changelog_event, upsert_asset_creators, upsert_asset_with_leaf_info,
        upsert_asset_with_owner_and_delegate_info, upsert_asset_with_seq,
    },
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use log::debug;
use mpl_bubblegum::types::Creator;
use sea_orm::{ConnectionTrait, TransactionTrait};

pub async fn process<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    instruction: &str,
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
                return Err(IngesterError::ParsingError(
                    "Ix not parsed correctly".to_string(),
                ));
            }
        };
        debug!(
            "Handling creator verification event for creator {} (verify: {}): {}",
            creator, verify, bundle.txn_id
        );
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, instruction, cl_audits)
            .await?;

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
            }
        };

        return Ok(());
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
