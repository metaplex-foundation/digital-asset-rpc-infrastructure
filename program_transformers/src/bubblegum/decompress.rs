use {
    crate::{
        bubblegum::db::{
            upsert_asset_with_compression_info, upsert_asset_with_leaf_info_for_decompression,
        },
        error::ProgramTransformerResult,
    },
    blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction},
    sea_orm::{ConnectionTrait, TransactionTrait},
};

pub async fn decompress<'c, T>(
    _parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    let id_bytes = bundle.keys.get(3).unwrap().0.as_slice();

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
