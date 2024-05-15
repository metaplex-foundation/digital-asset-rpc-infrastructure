use super::{audit, backfiller};
use anyhow::Result;
use clap::{Args, Subcommand};
use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
pub enum BubblegumOpsErrorKind {
    #[error("anchor")]
    Anchor(#[from] anchor_client::anchor_lang::error::Error),
    #[error("solana rpc")]
    Rpc(#[from] solana_client::client_error::ClientError),
    #[error("parse pubkey")]
    ParsePubkey(#[from] solana_sdk::pubkey::ParsePubkeyError),
    #[error("serialize tree response")]
    SerializeTreeResponse,
    #[error("sea orm")]
    Database(#[from] sea_orm::DbErr),
    #[error("try from pubkey")]
    TryFromPubkey,
    #[error("try from signature")]
    TryFromSignature,
    #[error("generic error: {0}")]
    Generic(String),
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    /// The 'backfill' command is used to cross-reference the index against on-chain accounts.
    /// It crawls through trees and backfills any missed tree transactions.
    #[clap(name = "backfill")]
    Backfill(backfiller::Args),
    /// The `audit` commands checks `cl_audits_v2` for any failed transactions and logs them to stdout.
    #[clap(name = "audit")]
    Audit(audit::Args),
}

#[derive(Debug, Clone, Args)]
pub struct BubblegumCommand {
    #[clap(subcommand)]
    pub action: Commands,
}

pub async fn subcommand(subcommand: BubblegumCommand) -> Result<()> {
    match subcommand.action {
        Commands::Backfill(args) => {
            backfiller::run(args).await?;
        }
        Commands::Audit(args) => {
            audit::run(args).await?;
        }
    }

    Ok(())
}
