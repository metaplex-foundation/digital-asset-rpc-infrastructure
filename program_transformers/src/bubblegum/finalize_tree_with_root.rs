use blockbuster::programs::bubblegum::Payload;
use digital_asset_types::dao::sea_orm_active_enums::BatchMintPersistingState;
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
    if let Some(Payload::FinalizeTreeWithRoot { args, .. }) = &parsing_result.payload {
        let query = digital_asset_types::dao::batch_mint_to_verify::Entity::insert(
            digital_asset_types::dao::batch_mint_to_verify::ActiveModel {
                file_hash: Set(args.metadata_hash.clone()),
                url: Set(args.metadata_url.clone()),
                created_at_slot: Set(bundle.slot as i64),
                signature: Set(bundle.txn_id.to_string()),
                staker: Set(args.staker.to_bytes().to_vec()),
                download_attempts: Set(0),
                batch_mint_persisting_state: Set(BatchMintPersistingState::ReceivedTransaction),
                batch_mint_fail_status: Set(None),
                collection: Set(args.collection_mint.map(|k| k.to_bytes().to_vec())),
            },
        )
        .on_conflict(
            OnConflict::columns([
                digital_asset_types::dao::batch_mint_to_verify::Column::FileHash,
                digital_asset_types::dao::batch_mint_to_verify::Column::Staker,
            ])
            .update_columns([digital_asset_types::dao::batch_mint_to_verify::Column::Url])
            .update_columns([digital_asset_types::dao::batch_mint_to_verify::Column::Signature])
            .update_columns([
                digital_asset_types::dao::batch_mint_to_verify::Column::DownloadAttempts,
            ])
            .update_columns([
                digital_asset_types::dao::batch_mint_to_verify::Column::BatchMintFailStatus,
            ])
            .update_columns([
                digital_asset_types::dao::batch_mint_to_verify::Column::BatchMintPersistingState,
            ])
            .update_columns([digital_asset_types::dao::batch_mint_to_verify::Column::CreatedAtSlot])
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