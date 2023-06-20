use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        upsert_asset_with_compression_info, upsert_asset_with_leaf_info,
    },
};
use blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction};
use sea_orm::{ConnectionTrait, TransactionTrait};

pub async fn decompress<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(cl) = &parsing_result.tree_update {
        let id_bytes = bundle.keys.get(3).unwrap().0.as_slice();

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

        // Partial update of asset table with just leaf.
        upsert_asset_with_leaf_info(txn, id_bytes.to_vec(), None, cl.seq as i64).await?;

        return Ok(());
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
