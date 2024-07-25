use blockbuster::programs::bubblegum::Payload;
use digital_asset_types::dao::sea_orm_active_enums::RollupPersistingState;
use sea_orm::sea_query::OnConflict;
use sea_orm::{DbBackend, EntityTrait, QueryTrait, Set};
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
        let query = digital_asset_types::dao::rollup_to_verify::Entity::insert(
            digital_asset_types::dao::rollup_to_verify::ActiveModel {
                file_hash: Set(args.metadata_hash.clone()),
                url: Set(args.metadata_url.clone()),
                created_at_slot: Set(bundle.slot as i64),
                signature: Set(bundle.txn_id.to_string()),
                staker: Set(args.staker.to_bytes().to_vec()),
                download_attempts: Set(0),
                rollup_persisting_state: Set(RollupPersistingState::ReceivedTransaction),
                rollup_fail_status: Set(None),
            },
        )
        .on_conflict(
            OnConflict::columns([
                digital_asset_types::dao::rollup_to_verify::Column::FileHash,
                digital_asset_types::dao::rollup_to_verify::Column::Staker,
            ])
            .update_columns([digital_asset_types::dao::rollup_to_verify::Column::Url])
            .update_columns([digital_asset_types::dao::rollup_to_verify::Column::Signature])
            .update_columns([digital_asset_types::dao::rollup_to_verify::Column::DownloadAttempts])
            .update_columns([digital_asset_types::dao::rollup_to_verify::Column::RollupFailStatus])
            .update_columns([
                digital_asset_types::dao::rollup_to_verify::Column::RollupPersistingState,
            ])
            .update_columns([digital_asset_types::dao::rollup_to_verify::Column::CreatedAtSlot])
            .to_owned(),
        )
        .build(DbBackend::Postgres);
        txn.execute(query)
            .await
            .map_err(|e| ProgramTransformerError::DatabaseError(e.to_string()))?;
        return Ok(());
    }

    Err(ProgramTransformerError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
