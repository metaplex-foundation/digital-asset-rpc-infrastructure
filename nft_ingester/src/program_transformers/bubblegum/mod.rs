use std::future::Future;
use std::pin::Pin;
use std::process::Output;
use cadence_macros::statsd_count;
use {
    crate::{
        error::IngesterError,
        events::handle_event,
        get_gummy_roll_events,
        parsers::{InstructionBundle, ProgramHandler, ProgramHandlerConfig},
        save_changelog_events,
        tasks::BgTask,
        utils::bytes_from_fb_table,
    },
    anchor_client::anchor_lang::{self, prelude::Pubkey, AnchorDeserialize},
    async_trait::async_trait,
    bubblegum::state::{
        leaf_schema::{LeafSchema, LeafSchemaEvent, Version},
        NFTDecompressionEvent,
    },
    digital_asset_types::{
        adapter::{TokenStandard, UseMethod, Uses},
        dao::{
            asset, asset_authority, asset_creators, asset_data, asset_grouping,
            sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
        },
        json::ChainDataV1,
    },
    flatbuffers::{ForwardsUOffset, Vector},
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
    bundle: &InstructionBundle<'a, 'b>,
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
        InstructionName::Burn => {
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
            let asset_data_oid = db.transaction::<_, u64, IngesterError>(|txn| {
                mint_v1::mint_v1(&parsing_result, bundle, txn)
            }).await?;
            let task = Some(DownloadMetadata {
                asset_data_id,
                uri: ix.message.uri.clone(),
            });
            task_manager.send(Box::new(task.unwrap()))?;
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
