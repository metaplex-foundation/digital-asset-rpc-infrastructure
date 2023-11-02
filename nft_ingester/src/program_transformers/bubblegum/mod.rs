use blockbuster::{
    self,
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, InstructionName},
};
use log::{debug, info};
use sea_orm::{ConnectionTrait, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

mod burn;
mod cancel_redeem;
mod collection_verification;
mod creator_verification;
mod db;
mod decompress;
mod delegate;
mod mint_v1;
mod redeem;
mod transfer;

pub use db::*;

use crate::{error::IngesterError, tasks::TaskData};

pub async fn handle_bubblegum_instruction<'c, T>(
    parsing_result: &'c BubblegumInstruction,
    bundle: &'c InstructionBundle<'c>,
    txn: &T,
    task_manager: &UnboundedSender<TaskData>,
    cl_audits: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let ix_type = &parsing_result.instruction;

    // @TODO this would be much better served by implemneting Debug trait on the InstructionName
    // or wrapping it into something that can display it more neatly.
    let ix_str = match ix_type {
        InstructionName::Unknown => "Unknown",
        InstructionName::MintV1 => "MintV1",
        InstructionName::MintToCollectionV1 => "MintToCollectionV1",
        InstructionName::Redeem => "Redeem",
        InstructionName::CancelRedeem => "CancelRedeem",
        InstructionName::Transfer => "Transfer",
        InstructionName::Delegate => "Delegate",
        InstructionName::DecompressV1 => "DecompressV1",
        InstructionName::Compress => "Compress",
        InstructionName::Burn => "Burn",
        InstructionName::CreateTree => "CreateTree",
        InstructionName::VerifyCreator => "VerifyCreator",
        InstructionName::UnverifyCreator => "UnverifyCreator",
        InstructionName::VerifyCollection => "VerifyCollection",
        InstructionName::UnverifyCollection => "UnverifyCollection",
        InstructionName::SetAndVerifyCollection => "SetAndVerifyCollection",
        InstructionName::SetDecompressibleState | InstructionName::UpdateMetadata => todo!(),
    };
    info!("BGUM instruction txn={:?}: {:?}", ix_str, bundle.txn_id);

    match ix_type {
        InstructionName::Transfer => {
            transfer::transfer(parsing_result, bundle, txn, cl_audits, ix_str).await?;
        }
        InstructionName::Burn => {
            burn::burn(parsing_result, bundle, txn, cl_audits, ix_str).await?;
        }
        InstructionName::Delegate => {
            delegate::delegate(parsing_result, bundle, txn, cl_audits, ix_str).await?;
        }
        InstructionName::MintV1 | InstructionName::MintToCollectionV1 => {
            let task = mint_v1::mint_v1(parsing_result, bundle, txn, cl_audits, ix_str).await?;

            if let Some(t) = task {
                task_manager.send(t)?;
            }
        }
        InstructionName::Redeem => {
            redeem::redeem(parsing_result, bundle, txn, cl_audits, ix_str).await?;
        }
        InstructionName::CancelRedeem => {
            cancel_redeem::cancel_redeem(parsing_result, bundle, txn, cl_audits, ix_str).await?;
        }
        InstructionName::DecompressV1 => {
            decompress::decompress(parsing_result, bundle, txn).await?;
        }
        InstructionName::VerifyCreator => {
            creator_verification::process(parsing_result, bundle, txn, true, cl_audits, ix_str).await?;
        }
        InstructionName::UnverifyCreator => {
            creator_verification::process(parsing_result, bundle, txn, false, cl_audits, ix_str).await?;
        }
        InstructionName::VerifyCollection
        | InstructionName::UnverifyCollection
        | InstructionName::SetAndVerifyCollection => {
            collection_verification::process(parsing_result, bundle, txn, cl_audits, ix_str).await?;
        }
        _ => debug!("Bubblegum: Not Implemented Instruction"),
    }
    Ok(())
}

// PDA lookup requires an 8-byte array.
fn u32_to_u8_array(value: u32) -> [u8; 8] {
    let bytes: [u8; 4] = value.to_le_bytes();
    let mut result: [u8; 8] = [0; 8];
    result[..4].copy_from_slice(&bytes);
    result
}
