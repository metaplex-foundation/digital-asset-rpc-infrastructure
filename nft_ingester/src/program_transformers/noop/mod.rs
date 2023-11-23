use blockbuster::{self, instruction::InstructionBundle, programs::noop::NoopInstruction};
// use log::info;
use sea_orm::{ConnectionTrait, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

mod db;

pub use db::*;

use crate::{error::IngesterError, tasks::TaskData};

pub async fn handle_noop_instruction<'c, T>(
    parsing_result: &'c NoopInstruction,
    _bundle: &'c InstructionBundle<'c>,
    txn: &T,
    _task_manager: &UnboundedSender<TaskData>,
    _cl_audits: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let Some(app) = &parsing_result.application_data {
        let _seq = save_applicationdata_event(app, txn).await?;
    }
    Ok(())
}

// PDA lookup requires an 8-byte array.
fn _u32_to_u8_array(value: u32) -> [u8; 8] {
    let bytes: [u8; 4] = value.to_le_bytes();
    let mut result: [u8; 8] = [0; 8];
    result[..4].copy_from_slice(&bytes);
    result
}
