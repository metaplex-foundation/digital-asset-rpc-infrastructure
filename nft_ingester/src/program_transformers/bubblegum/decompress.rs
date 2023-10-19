use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        asset_was_decompressed, upsert_asset_with_compression_info,
        upsert_asset_with_leaf_info_for_decompression,
    },
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

    // First check to see if this asset has been decompressed and if so do not update.
    if asset_was_decompressed(txn, id_bytes.to_vec()).await? {
        return Ok(());
    }

    // Partial update of asset table with just leaf.
    upsert_asset_with_leaf_info_for_decompression(txn, id_bytes.to_vec()).await?;

    upsert_asset_with_compression_info(
        txn,
        id_bytes.to_vec(),
        false,
        false,
        1,
        Some(id_bytes.to_vec()),
        true,
    )
    .await
}
