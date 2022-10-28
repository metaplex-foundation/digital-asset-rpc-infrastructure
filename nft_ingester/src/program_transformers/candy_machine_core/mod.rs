use blockbuster::{self, programs::candy_machine_core::CandyMachineCoreAccountData};

use plerkle_serialization::AccountInfo;
use sea_orm::{DatabaseConnection, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

use crate::{BgTask, IngesterError};

pub mod candy_machine_core;

pub async fn handle_candy_machine_core_account_update<'a, 'b, 'c>(
    account_update: &'a AccountInfo<'a>,
    parsing_result: &'b CandyMachineCoreAccountData,
    db: &'c DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let txn = db.begin().await?;
    let key = account_update.pubkey().unwrap().clone();
    match parsing_result {
        CandyMachineCoreAccountData::CandyMachineCore(candy_machine_core) => {
            candy_machine_core::candy_machine_core(candy_machine_core, key, &txn, &db).await?;
            txn.commit().await?;
        }
        _ => println!("Candy Machine Core: Account update invalid."),
    }

    Ok(())
}
