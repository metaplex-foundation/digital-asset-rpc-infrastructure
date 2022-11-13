use blockbuster::programs::candy_machine::CandyMachineAccountData;

use plerkle_serialization::AccountInfo;
use sea_orm::{DatabaseConnection, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

mod candy_machine;
mod collections;
mod freeze;

use crate::{tasks::TaskData, IngesterError};

pub async fn handle_candy_machine_account_update(
    account_update: &AccountInfo<'_>,
    parsing_result: &CandyMachineAccountData,
    db: &DatabaseConnection,
    task_manager: &UnboundedSender<TaskData>,
) -> Result<(), IngesterError> {
    let txn = db.begin().await?;
    let key = account_update.pubkey().unwrap().clone();
    match parsing_result {
        CandyMachineAccountData::CandyMachine(candy_machine) => {
            candy_machine::candy_machine(candy_machine, key, &txn, &db).await?;
            txn.commit().await?;
        }
        CandyMachineAccountData::CollectionPDA(collection_pda) => {
            collections::collections(collection_pda, key, &txn).await?;
            txn.commit().await?;
        }
        CandyMachineAccountData::FreezePDA(freeze_pda) => {
            freeze::freeze(freeze_pda, key, &txn).await?;
            txn.commit().await?;
        }
        _ => println!("Candy Machine: Account update invalid."),
    }
    Ok(())
}
