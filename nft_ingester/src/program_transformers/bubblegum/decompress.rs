use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        asset_should_be_updated, upsert_asset_with_compression_info,
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

    // First check to see if this asset has been decompressed.
    if !asset_should_be_updated(txn, id_bytes.to_vec(), None).await? {
        return Ok(());
    }

    // Start a db transaction.
    let multi_txn = txn.begin().await?;

    // Partial update of asset table with just leaf.
    upsert_asset_with_leaf_info_for_decompression(&multi_txn, id_bytes.to_vec()).await?;

    upsert_asset_with_compression_info(
        &multi_txn,
        id_bytes.to_vec(),
        false,
        false,
        1,
        Some(id_bytes.to_vec()),
        true,
    )
    .await?;

    // Close out transaction and relinqish the lock.
    multi_txn.commit().await?;

    Ok(())
}
