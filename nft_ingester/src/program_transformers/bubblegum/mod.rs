use blockbuster::{
    self,
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, InstructionName},
};
use log::debug;
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
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let ix_type = &parsing_result.instruction;
    match ix_type {
        InstructionName::Unknown => {
            debug!("Unknown instruction:");
        }
        InstructionName::MintV1 => {
            debug!("MintV1 instruction:");
        }
        InstructionName::MintToCollectionV1 => {
            debug!("MintToCollectionV1 instruction:");
        }
        InstructionName::Redeem => {
            debug!("Redeem instruction:");
        }
        InstructionName::CancelRedeem => {
            debug!("CancelRedeem instruction:");
        }
        InstructionName::Transfer => {
            debug!("Transfer instruction:");
        }
        InstructionName::Delegate => {
            debug!("Delegate instruction:");
        }
        InstructionName::DecompressV1 => {
            debug!("DecompressV1 instruction:");
        }
        InstructionName::Compress => {
            debug!("Compress instruction:");
        }
        InstructionName::Burn => {
            debug!("Burn instruction:");
        }
        InstructionName::CreateTree => {
            debug!("CreateTree instruction:");
        }
        InstructionName::VerifyCreator => {
            debug!("VerifyCreator instruction:");
        }
        InstructionName::UnverifyCreator => {
            debug!("UnverifyCreator instruction:");
        }
        InstructionName::VerifyCollection => {
            debug!("VerifyCollection instruction:");
        }
        InstructionName::UnverifyCollection => {
            debug!("UnverifyCollection instruction:");
        }
        InstructionName::SetAndVerifyCollection => {
            debug!("SetAndVerifyCollection instruction:");
        }
    }

    match ix_type {
        InstructionName::Transfer => {
            transfer::transfer(parsing_result, bundle, txn).await?;
        }
        InstructionName::Burn => {
            burn::burn(parsing_result, bundle, txn).await?;
        }
        InstructionName::Delegate => {
            delegate::delegate(parsing_result, bundle, txn).await?;
        }
        InstructionName::MintV1 | InstructionName::MintToCollectionV1 => {
            let task = mint_v1::mint_v1(parsing_result, bundle, txn).await?;

            task_manager.send(task)?;
        }
        InstructionName::Redeem => {
            redeem::redeem(parsing_result, bundle, txn).await?;
        }
        InstructionName::CancelRedeem => {
            cancel_redeem::cancel_redeem(parsing_result, bundle, txn).await?;
        }
        InstructionName::DecompressV1 => {
            decompress::decompress(parsing_result, bundle, txn).await?;
        }
        InstructionName::VerifyCreator => {
            creator_verification::process(parsing_result, bundle, txn, true).await?;
        }
        InstructionName::UnverifyCreator => {
            creator_verification::process(parsing_result, bundle, txn, false).await?;
        }
        InstructionName::VerifyCollection => {
            collection_verification::process(parsing_result, bundle, txn, true).await?;
        }
        InstructionName::UnverifyCollection => {
            collection_verification::process(parsing_result, bundle, txn, false).await?;
        }
        InstructionName::SetAndVerifyCollection => {
            collection_verification::process(parsing_result, bundle, txn, true).await?;
        }
        _ => debug!("Bubblegum: Not Implemented Instruction"),
    }
    Ok(())
}
