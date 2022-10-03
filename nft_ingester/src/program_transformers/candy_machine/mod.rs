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

mod candy_machine;
mod collections;
mod freeze;
mod state;

pub use db::*;

use crate::{BgTask, IngesterError};

pub async fn handle_candy_machine_account_update<'a, 'b, 'c>(
    account_update: &'a AccountInfo<'a>,
    parsing_result: &'b CandyMachineAccountData,
    db: &'c DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let txn = db.begin().await?;
    match parsing_result {
        CandyMachineAccountData::CandyMachine(candy_machine) => {
            candy_machine::candy_machine(candy_machine, acct, &txn).await?;
            txn.commit().await?;
        }
        CandyMachineAccountData::CollectionPDA(collection_pda) => {
            collections::collections(collection_pda, acct, &txn).await?;
            txn.commit().await?;
        }
        CandyMachineAccountData::FreezePDA(freeze_pda) => {
            freeze::freeze(freeze_pda, acct, &txn).await?;
            txn.commit().await?;
        }
        _ => println!("Candy Machine: Account update invalid."),
    }
    Ok(())
}
