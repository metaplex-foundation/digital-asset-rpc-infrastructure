use blockbuster::{
    self,
    instruction::InstructionBundle,
    programs::candy_machine_core::{CandyMachineCoreAccountData, InstructionName},
};

use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{entity::*, DatabaseConnection, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

use crate::{BgTask, IngesterError};

pub mod candy_machine_core;

pub async fn handle_candy_machine_core_account_update<'c>(
    parsing_result: &'c CandyMachineCoreAccountData,
    acct: &'c AccountInfo<'c>,
    db: &DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let txn = db.begin().await?;
    match parsing_result {
        CandyMachineCoreAccountData::CandyMachineCore(candy_machine_core) => {
            candy_machine_core::candy_machine_core(candy_machine_core, acct, &txn).await?;
            txn.commit().await?;
        }
        _ => println!("Candy Machine Core: Account update invalid."),
    }

    Ok(())
}
