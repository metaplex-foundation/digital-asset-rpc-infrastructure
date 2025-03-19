use {
    crate::{
        bubblegum::{
            db::{save_changelog_event, upsert_asset_with_leaf_info, upsert_asset_with_seq},
            u32_to_u8_array,
        },
        error::{ProgramTransformerError, ProgramTransformerResult},
    },
    blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction},
    sea_orm::{ConnectionTrait, TransactionTrait},
    solana_sdk::pubkey::Pubkey,
    tracing::debug,
};

pub async fn redeem<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    instruction: &str,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(cl) = &parsing_result.tree_update {
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, instruction).await?;
        let leaf_index = cl.index;
        let (asset_id, _) = Pubkey::find_program_address(
            &[
                "asset".as_bytes(),
                cl.id.as_ref(),
                u32_to_u8_array(leaf_index).as_ref(),
            ],
            &mpl_bubblegum::ID,
        );
        debug!("Indexing redeem for asset id: {:?}", asset_id);
        let id_bytes = asset_id.to_bytes();
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
            vec![0; 32],
            [0; 32],
            [0; 32],
            None,
            None,
            seq as i64,
        )
        .await?;

        upsert_asset_with_seq(&multi_txn, id_bytes.to_vec(), seq as i64).await?;

        multi_txn.commit().await?;

        return Ok(());
    }
    Err(ProgramTransformerError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
