use blockbuster::{
    self,
    instruction::InstructionBundle,
    programs::candy_machine::{CandyMachineAccountData, InstructionName},
};
use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{entity::*, DatabaseConnection, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

pub use db::*;

use crate::{BgTask, IngesterError};

pub async fn handle_candy_machine_account_update<'c>(
    parsing_result: &'c CandyMachineAccountData,
    acct: &'c AccountInfo<'c>,
    db: &DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let ix_type = &parsing_result.instruction;
    let txn = db.begin().await?;
    match ix_type {
        InstructionName::Transfer => {
            transfer::transfer(parsing_result, bundle, &txn).await?;
            txn.commit().await?;
        }
        InstructionName::Burn => {
            burn::burn(parsing_result, bundle, &txn).await?;
            txn.commit().await?;
        }
        InstructionName::Delegate => {
            delegate::delegate(parsing_result, bundle, &txn).await?;
        }
        InstructionName::MintV1 => {
            let task = mint_v1::mint_v1(parsing_result, bundle, &txn).await?;
            txn.commit().await?;
            task_manager.send(Box::new(task))?;
        }
        InstructionName::Redeem => {
            redeem::redeem(parsing_result, bundle, &txn).await?;
            txn.commit().await?;
        }
        InstructionName::CancelRedeem => {
            cancel_redeem::cancel_redeem(parsing_result, bundle, &txn).await?;
            txn.commit().await?;
        }
        InstructionName::DecompressV1 => {
            decompress::decompress(parsing_result, bundle, &txn).await?;
            txn.commit().await?;
        }
        _ => println!("Bubblegum: Not Implemented Instruction"),
    }
    Ok(())
}
