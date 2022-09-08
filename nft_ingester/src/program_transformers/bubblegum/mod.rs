use {
    plerkle_serialization::transaction_info_generated::transaction_info::{self},
    sea_orm::{
        entity::*, query::*, sea_query::OnConflict, DatabaseConnection, DatabaseTransaction,
        DbBackend, DbErr, JsonValue, SqlxPostgresConnector, TransactionTrait,
    },
    serde_json, solana_sdk,
    solana_sdk::pubkeys,
    sqlx::{self, Pool, Postgres},
    std::fmt::{Display, Formatter},
    tokio::sync::mpsc::UnboundedSender,
    blockbuster,
};
use blockbuster::instruction::InstructionBundle;
use blockbuster::programs::bubblegum::{BubblegumInstruction, InstructionName};
use crate::program_transformers::bubblegum::task::DownloadMetadata;

mod transfer;
mod burn;
mod task;
mod delegate;
mod mint_v1;
mod redeem;
mod cancel_redeem;
mod decompress;
mod db;

pub use db::*;

use crate::{BgTask, IngesterError};


pub async fn handle_bubblegum_instruction<'c>(
    parsing_result: &'c BubblegumInstruction,
    bundle: &'c InstructionBundle<'c>,
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
