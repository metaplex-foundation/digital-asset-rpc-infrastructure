use anchor_lang::prelude::Pubkey;
use log::debug;

use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        asset_should_be_updated, save_changelog_event, u32_to_u8_array,
        upsert_asset_with_leaf_info, upsert_asset_with_seq,
    },
};
use blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction};
use sea_orm::{ConnectionTrait, TransactionTrait};

pub async fn redeem<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    cl_audits: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(cl) = &parsing_result.tree_update {
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, cl_audits).await?;
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

        // First check to see if this asset has been decompressed or updated by
        // `update_metadata`.
        if !asset_should_be_updated(txn, id_bytes.to_vec(), Some(seq as i64)).await? {
            return Ok(());
        }

        let tree_id = cl.id.to_bytes();
        let nonce = cl.index as i64;

        // Partial update of asset table with just leaf.
        upsert_asset_with_leaf_info(
            txn,
            id_bytes.to_vec(),
            nonce,
            tree_id.to_vec(),
            vec![0; 32],
            [0; 32],
            [0; 32],
            seq as i64,
        )
        .await?;

        upsert_asset_with_seq(txn, id_bytes.to_vec(), seq as i64).await?;

        return Ok(());
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
