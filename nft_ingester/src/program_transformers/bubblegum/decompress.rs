use crate::{
    error::IngesterError,
    program_transformers::bubblegum::upsert_asset_with_leaf_and_compression_info_for_decompression,
};
use blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction};
use sea_orm::{query::*, ConnectionTrait};

pub async fn decompress<'c, T>(
    _parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let id_bytes = bundle.keys.get(3).unwrap().0.as_slice();

    // Partial update of asset table with leaf and compression info.
    upsert_asset_with_leaf_and_compression_info_for_decompression(txn, id_bytes.to_vec()).await?;

    Ok(())
}
