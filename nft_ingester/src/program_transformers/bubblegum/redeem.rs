use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        save_changelog_event, u32_to_u8_array, upsert_asset_with_leaf_info,
    },
};
use anchor_lang::prelude::Pubkey;
use blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction};
use log::debug;
use sea_orm::{ConnectionTrait, TransactionTrait};

pub async fn redeem<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(cl) = &parsing_result.tree_update {
        let seq = save_changelog_event(cl, bundle.slot, txn).await?;
        let leaf_index = cl.index;
        let (asset_id, _) = Pubkey::find_program_address(
            &[
                "asset".as_bytes(),
                cl.id.as_ref(),
                u32_to_u8_array(leaf_index).as_ref(),
            ],
            &mpl_bubblegum::ID,
        );
        debug!("Indexing burn for asset id: {:?}", asset_id);
        let id_bytes = asset_id.to_bytes();

        // Partial update of asset table with just leaf.
        upsert_asset_with_leaf_info(txn, id_bytes.to_vec(), Some(vec![0; 32]), Some(seq as i64))
            .await?;

        return Ok(());
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
