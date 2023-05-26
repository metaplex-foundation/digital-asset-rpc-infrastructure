use super::{upsert_asset_with_compression_info, upsert_asset_with_leaf_schema};
use crate::error::IngesterError;
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema},
};
use sea_orm::{ConnectionTrait, TransactionTrait};

pub async fn decompress<'c, T>(
    parsing_result: &BubblegumInstruction,
    _bundle: &InstructionBundle<'c>,
    txn: &'c T,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (Some(le), Some(cl)) = (&parsing_result.leaf_update, &parsing_result.tree_update) {
        #[allow(unreachable_patterns)]
        return match le.schema {
            LeafSchema::V1 {
                id,
                delegate,
                owner,
                ..
            } => {
                let id_bytes = id.to_bytes();
                upsert_asset_with_compression_info(
                    txn,
                    id_bytes.to_vec(),
                    false,
                    false,
                    1,
                    Some(id_bytes.to_vec()),
                    cl.seq as i64,
                )
                .await?;

                // Partial update of asset table with just leaf schema elements.
                let delegate = if owner == delegate {
                    None
                } else {
                    Some(delegate.to_bytes().to_vec())
                };
                let owner_bytes = owner.to_bytes().to_vec();
                upsert_asset_with_leaf_schema(
                    txn,
                    id_bytes.to_vec(),
                    le.leaf_hash.to_vec(),
                    delegate,
                    owner_bytes,
                    cl.seq as i64,
                )
                .await
            }
            _ => Err(IngesterError::NotImplemented),
        };
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
