use blockbuster::{
    self,
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, InstructionName},
};
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
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

use crate::{IngesterError, TaskData};

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
            println!("Unknown instruction:");
        }
        InstructionName::MintV1 => {
            println!("MintV1 instruction:");
        }
        InstructionName::MintToCollectionV1 => {
            println!("MintToCollectionV1 instruction:");
        }
        InstructionName::Redeem => {
            println!("Redeem instruction:");
        }
        InstructionName::CancelRedeem => {
            println!("CancelRedeem instruction:");
        }
        InstructionName::Transfer => {
            println!("Transfer instruction:");
        }
        InstructionName::Delegate => {
            println!("Delegate instruction:");
        }
        InstructionName::DecompressV1 => {
            println!("DecompressV1 instruction:");
        }
        InstructionName::Compress => {
            println!("Compress instruction:");
        }
        InstructionName::Burn => {
            println!("Burn instruction:");
        }
        InstructionName::CreateTree => {
            println!("CreateTree instruction:");
        }
        InstructionName::VerifyCreator => {
            println!("VerifyCreator instruction:");
        }
        InstructionName::UnverifyCreator => {
            println!("UnverifyCreator instruction:");
        }
        InstructionName::VerifyCollection => {
            println!("VerifyCollection instruction:");
        }
        InstructionName::UnverifyCollection => {
            println!("UnverifyCollection instruction:");
        }
        InstructionName::SetAndVerifyCollection => {
            println!("SetAndVerifyCollection instruction:");
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
        _ => println!("Bubblegum: Not Implemented Instruction"),
    }
    Ok(())
}
