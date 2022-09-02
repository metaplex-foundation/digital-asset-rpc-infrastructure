use std::future::Future;
use std::pin::Pin;
use std::process::Output;
use cadence_macros::statsd_count;
use {
    crate::{
        error::IngesterError,
        parsers::{InstructionBundle, ProgramHandler, ProgramHandlerConfig},
        save_changelog_events,
        tasks::BgTask,
    },
    lazy_static::lazy_static,
    num_traits::FromPrimitive,
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
use crate::program_transformers::common::save_changelog_event;
use crate::Pubkey;

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


pub async fn handle_bubblegum_instruction<'a, 'b, 't>(
    parsing_result: BubblegumInstruction,
    bundle: &InstructionBundle<'a>,
    db: &DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let ix_type = parsing_result.instruction;
    match ix_type {
        InstructionName::Transfer => {
            db.transaction::<_, _, IngesterError>(move |txn| {
                transfer::transfer(&parsing_result, bundle, txn)
            }).await?;
        }
        InstructionName::Burn =>
            {
            db.transaction::<_, _, IngesterError>(|txn| {
                burn::burn(&parsing_result, bundle, txn)
            })
                .await?;
        }
        InstructionName::Delegate => {
            db.transaction::<_, _, IngesterError>(|txn| {
                delegate::delegate(&parsing_result, bundle, txn)
            })
                .await?;
        }
        InstructionName::MintV1 => {
            let task = db.transaction::<_, DownloadMetadata, IngesterError>(|txn| {
                mint_v1::mint_v1(&parsing_result, bundle, txn)
            }).await?;
            task_manager.send(Box::new(task))?;
        }
        InstructionName::Redeem => {
            db.transaction::<_, _, IngesterError>(|txn| {
                redeem::redeem(&parsing_result, bundle, txn)
            })
                .await?;
        }
        InstructionName::CancelRedeem => {
            db.transaction::<_, _, IngesterError>(|txn| {
                cancel_redeem::cancel_redeem(&parsing_result, bundle, txn)
            })
                .await?;
        }
        InstructionName::DecompressV1 => {
            db.transaction::<_, _, IngesterError>(|txn| {
                decompress::decompress(&parsing_result, bundle, txn)
            })
                .await?;
        }
        _ => println!("Bubblegum: Not Implemented Instruction"),
    }
    Ok(())
}
