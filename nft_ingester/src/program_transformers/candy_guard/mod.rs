use blockbuster::{
    self,
    instruction::InstructionBundle,
    programs::candy_guard::{CandyGuardAccountData, InstructionName},
};

use mpl_candy_guard::state::{CandyGuard, CandyGuardData};

use sea_orm::{ DatabaseConnection, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

use crate::{BgTask, IngesterError};

mod candy_guard;
mod helpers;

pub async fn handle_candy_guard_account_update<'a, 'b, 'c>(
    account_update: &'a AccountInfo<'a>,
    parsing_result: &'b CandyGuardAccountData,
    db: &'c DatabaseConnection,
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
        // TODO mint counter :(
        _ => println!("Candy Machine Guard: Account update invalid."),
    }

    Ok(())
}
