use blockbuster::programs::candy_guard::CandyGuardAccountData;

use plerkle_serialization::AccountInfo;
use sea_orm::{DatabaseConnection, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

use crate::{BgTask, IngesterError};

mod candy_guard;
mod helpers;

pub async fn handle_candy_guard_account_update(
    account_update: &AccountInfo<'_>,
    parsing_result: &CandyGuardAccountData,
    db: &DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let txn = db.begin().await?;
    let key = account_update.pubkey().unwrap().clone();
    match parsing_result {
        CandyGuardAccountData::CandyGuard(candy_guard, candy_guard_data) => {
            candy_guard::candy_guard(candy_guard, candy_guard_data, key, &txn, &db).await?;
            txn.commit().await?;
        }
        // CandyGuardAccountData::MintCounter(mint_counter) => {
        //     mint_counter::mint_counter(mint_counter, acct, &txn).await?;
        //     txn.commit().await?;
        // }
        // TODO mint counter :( P-688
        _ => println!("Candy Machine Guard: Account update invalid."),
    }

    Ok(())
}
