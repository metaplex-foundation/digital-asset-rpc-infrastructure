use crate::bubblegum;
use crate::bubblegum::rollup_persister::Rollup;
use blockbuster::programs::bubblegum::Payload;
use digital_asset_types::dao::sea_orm_active_enums::RollupPersistingState;
use sea_orm::{ActiveModelTrait, Set};
use solana_sdk::signature::Signature;
use std::str::FromStr;
use {
    crate::error::{ProgramTransformerError, ProgramTransformerResult},
    blockbuster::{instruction::InstructionBundle, programs::bubblegum::BubblegumInstruction},
    sea_orm::{query::QueryTrait, ConnectionTrait, TransactionTrait},
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
            signature: Set(Signature::from_str(bundle.txn_id)
                .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?
                .into()),
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

pub async fn store_rollup_update<'c, T>(
    slot: u64,
    signature: Signature,
    rollup: &Rollup,
    txn: &'c T,
    cl_audits: bool,
) -> ProgramTransformerResult<()>
where
    T: ConnectionTrait + TransactionTrait,
{
    for rolled_mint in rollup.rolled_mints.iter() {
        bubblegum::mint_v1::mint_v1(
            &rolled_mint.into(),
            &InstructionBundle {
                txn_id: &signature.to_string(),
                program: Default::default(),
                instruction: None,
                inner_ix: None,
                keys: &[],
                slot,
            },
            txn,
            "CreateTreeWithRoot",
            cl_audits,
        )?;
    }

    Ok(())
}
