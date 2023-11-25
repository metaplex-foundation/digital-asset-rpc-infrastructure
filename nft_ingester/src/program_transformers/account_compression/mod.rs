use blockbuster::{
    self,
    instruction::InstructionBundle,
    programs::account_compression::{AccountCompressionInstruction, Instruction},
};
use log::{debug, info};
use sea_orm::{ConnectionTrait, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

mod append;
mod close_tree;
mod db;
mod init_tree;
mod insert_or_append;
mod replace_leaf;
mod transfer_authority;
mod verify_leaf;

pub use db::*;

use crate::{error::IngesterError, tasks::TaskData};

pub async fn handle_account_compression_instruction<'c, T>(
    parsing_result: &'c AccountCompressionInstruction,
    bundle: &'c InstructionBundle<'c>,
    txn: &T,
    _task_manager: &UnboundedSender<TaskData>,
    cl_audits: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let ix_type = &parsing_result.instruction;

    // @TODO this would be much better served by implemneting Debug trait on the Instruction
    // or wrapping it into something that can display it more neatly.
    debug!("AccountCompressionInstruction ix_type: {:#?}", ix_type);
    let ix_str = match ix_type {
        Instruction::Unknown => "Unknown",
        Instruction::InitTree { .. } => {
            init_tree::init_tree(parsing_result, bundle, txn, cl_audits).await?;
            "InitTree"
        }
        Instruction::ReplaceLeaf { .. } => {
            replace_leaf::replace_leaf(parsing_result, bundle, txn, cl_audits).await?;
            "ReplaceLeaf"
        }
        Instruction::TransferAuthority { .. } => {
            transfer_authority::transfer_authority(parsing_result, bundle, txn, cl_audits).await?;
            "TransferAuthority"
        }
        Instruction::VerifyLeaf { .. } => {
            verify_leaf::verify_leaf(parsing_result, bundle, txn, cl_audits).await?;
            "VerifyLeaf"
        }
        Instruction::Append { .. } => {
            append::append(parsing_result, bundle, txn, cl_audits).await?;
            "Append"
        }
        Instruction::InsertOrAppend { .. } => {
            insert_or_append::insert_or_append(parsing_result, bundle, txn, cl_audits).await?;
            "InsertOrAppend"
        }
        Instruction::CloseTree => {
            close_tree::close_tree(parsing_result, bundle, txn, cl_audits).await?;
            "CloseTree"
        }
    };
    info!("CMT instruction txn={:?}: {:?}", ix_str, bundle.txn_id);
    Ok(())
}

// PDA lookup requires an 8-byte array.
fn _u32_to_u8_array(value: u32) -> [u8; 8] {
    let bytes: [u8; 4] = value.to_le_bytes();
    let mut result: [u8; 8] = [0; 8];
    result[..4].copy_from_slice(&bytes);
    result
}
