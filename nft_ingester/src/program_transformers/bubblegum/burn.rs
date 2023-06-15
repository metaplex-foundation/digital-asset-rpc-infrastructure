use super::save_changelog_event;
use crate::error::IngesterError;
use anchor_lang::prelude::Pubkey;
use blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction};
use digital_asset_types::dao::asset;
use log::debug;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DbBackend, EntityTrait,
    TransactionTrait,
};

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

        let asset_model = asset::ActiveModel {
            id: Set(id_bytes.clone()),
            burnt: Set(true),
            ..Default::default()
        };

        // Upsert asset table `burnt` column.
        let query = asset::Entity::insert(asset_model)
            .on_conflict(
                OnConflict::columns([asset::Column::Id])
                    .update_columns([
                        asset::Column::Burnt,
                        //TODO maybe handle slot updated.
                    ])
                    .to_owned(),
            )
            .build(DbBackend::Postgres);
        txn.execute(query).await?;
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
