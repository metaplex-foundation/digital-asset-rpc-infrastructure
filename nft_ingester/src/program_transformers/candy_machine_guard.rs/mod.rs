use blockbuster::{
    self,
    instruction::InstructionBundle,
    programs::candy_guard::{CandyGuardAccountData, InstructionName},
};

use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{entity::*, DatabaseConnection, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

use crate::{BgTask, IngesterError};

pub async fn handle_candy_guard_account_update<'c>(
    parsing_result: &'c CandyGuardAccountData,
    acct: &'c AccountInfo<'c>,
    db: &DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let txn = db.begin().await?;
    match parsing_result {
        CandyGuardAccountData::CandyGuard(candy_guard, candy_guard_data) => {
            candy_guard::candy_guard(candy_guard, candy_guard_data, acct, &txn).await?;
            txn.commit().await?;
        }
        CandyGuardAccountData::MintCounter(mint_counter) => {
            mint_counter::mint_counter(mint_counter, acct, &txn).await?;
            txn.commit().await?;
        }
        _ => println!("Candy Machine Guard: Account update invalid."),
    }

    Ok(())
}
