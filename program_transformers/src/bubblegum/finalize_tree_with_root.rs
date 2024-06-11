use blockbuster::programs::bubblegum::Payload;
use digital_asset_types::dao::sea_orm_active_enums::RollupPersistingState;
use sea_orm::{ActiveModelTrait, Set};
use {
    crate::error::{ProgramTransformerError, ProgramTransformerResult},
    blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction},
    sea_orm::{ConnectionTrait, TransactionTrait},
};

pub async fn finalize_tree_with_root<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(Payload::CreateTreeWithRoot { args, .. }) = &parsing_result.payload {
        digital_asset_types::dao::rollup_to_verify::ActiveModel {
            file_hash: Set(args.metadata_hash.clone()),
            url: Set(args.metadata_url.clone()),
            created_at_slot: Set(bundle.slot as i64),
            signature: Set(bundle.txn_id.to_string()),
            download_attempts: Set(0),
            rollup_persisting_state: Set(RollupPersistingState::ReceivedTransaction),
            rollup_fail_status: Set(None),
        }
        .insert(txn)
        .await
        .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
        return Ok(());
    }

    Err(ProgramTransformerError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
