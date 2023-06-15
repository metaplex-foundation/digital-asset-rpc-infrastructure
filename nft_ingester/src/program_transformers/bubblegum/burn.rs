use super::{save_changelog_event, update_asset};
use crate::error::IngesterError;
use anchor_lang::prelude::Pubkey;
use blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction};
use digital_asset_types::dao::asset;
use log::debug;
use sea_orm::{entity::*, ConnectionTrait, TransactionTrait};

pub async fn burn<'c, T>(
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
        let id_bytes = asset_id.to_bytes().to_vec();
        let asset_to_update = asset::ActiveModel {
            id: Unchanged(id_bytes.clone()),
            burnt: Set(true),
            seq: Set(seq as i64),
            ..Default::default()
        };
        // Don't send sequence number with this update, because we will always
        // run this update even if it's from a backfill/replay.
        update_asset(txn, id_bytes, None, asset_to_update).await?;
        return Ok(());
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}

// PDA lookup requires an 8-byte array.
fn u32_to_u8_array(value: u32) -> [u8; 8] {
    let bytes: [u8; 4] = value.to_le_bytes();
    let mut result: [u8; 8] = [0; 8];
    result[..4].copy_from_slice(&bytes);
    result
}
