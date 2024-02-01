use blockbuster::{
    self,
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, InstructionName, UseMethod as BubblegumUseMethod},
    token_metadata::types::UseMethod as TokenMetadataUseMethod,
};
use log::{debug, info};
use sea_orm::{ConnectionTrait, TransactionTrait};
use tokio::sync::mpsc::UnboundedSender;

mod burn;
mod cancel_redeem;
mod collection_verification;
mod creator_verification;
mod db;
mod delegate;
mod mint_v1;
mod redeem;
mod transfer;
mod update_metadata;

pub use db::*;

use crate::{error::IngesterError, tasks::TaskData};

pub async fn handle_bubblegum_instruction<'c, T>(
    parsing_result: &'c BubblegumInstruction,
    bundle: &'c InstructionBundle<'c>,
    txn: &T,
    task_manager: &UnboundedSender<TaskData>,
    cl_audits: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let ix_type = &parsing_result.instruction;

    // @TODO this would be much better served by implemneting Debug trait on the InstructionName
    // or wrapping it into something that can display it more neatly.
    let ix_str = match ix_type {
        InstructionName::Unknown => "Unknown",
        InstructionName::MintV1 => "MintV1",
        InstructionName::MintToCollectionV1 => "MintToCollectionV1",
        InstructionName::Redeem => "Redeem",
        InstructionName::CancelRedeem => "CancelRedeem",
        InstructionName::Transfer => "Transfer",
        InstructionName::Delegate => "Delegate",
        InstructionName::DecompressV1 => "DecompressV1",
        InstructionName::Compress => "Compress",
        InstructionName::Burn => "Burn",
        InstructionName::CreateTree => "CreateTree",
        InstructionName::VerifyCreator => "VerifyCreator",
        InstructionName::UnverifyCreator => "UnverifyCreator",
        InstructionName::VerifyCollection => "VerifyCollection",
        InstructionName::UnverifyCollection => "UnverifyCollection",
        InstructionName::SetAndVerifyCollection => "SetAndVerifyCollection",
        InstructionName::SetDecompressibleState => "SetDecompressibleState",
        InstructionName::UpdateMetadata => "UpdateMetadata",
    };
    info!("BGUM instruction txn={:?}: {:?}", ix_str, bundle.txn_id);

    match ix_type {
        InstructionName::Transfer => {
            transfer::transfer(parsing_result, bundle, txn, ix_str, cl_audits).await?;
        }
        InstructionName::Burn => {
            burn::burn(parsing_result, bundle, txn, ix_str, cl_audits).await?;
        }
        InstructionName::Delegate => {
            delegate::delegate(parsing_result, bundle, txn, ix_str, cl_audits).await?;
        }
        InstructionName::MintV1 | InstructionName::MintToCollectionV1 => {
            let task = mint_v1::mint_v1(parsing_result, bundle, txn, ix_str, cl_audits).await?;

            if let Some(t) = task {
                task_manager.send(t)?;
            }
        }
        InstructionName::Redeem => {
            redeem::redeem(parsing_result, bundle, txn, ix_str, cl_audits).await?;
        }
        InstructionName::CancelRedeem => {
            cancel_redeem::cancel_redeem(parsing_result, bundle, txn, ix_str, cl_audits).await?;
        }
        InstructionName::DecompressV1 => {
            debug!("No action necessary for decompression")
        }
        InstructionName::VerifyCreator | InstructionName::UnverifyCreator => {
            creator_verification::process(parsing_result, bundle, txn, ix_str, cl_audits).await?;
        }
        InstructionName::VerifyCollection
        | InstructionName::UnverifyCollection
        | InstructionName::SetAndVerifyCollection => {
            collection_verification::process(parsing_result, bundle, txn, ix_str, cl_audits)
                .await?;
        }
        InstructionName::SetDecompressibleState => (), // Nothing to index.
        InstructionName::UpdateMetadata => {
            let task =
                update_metadata::update_metadata(parsing_result, bundle, txn, ix_str, cl_audits)
                    .await?;

            if let Some(t) = task {
                task_manager.send(t)?;
            }
        }
        _ => debug!("Bubblegum: Not Implemented Instruction"),
    }
    Ok(())
}

// PDA lookup requires an 8-byte array.
fn u32_to_u8_array(value: u32) -> [u8; 8] {
    let bytes: [u8; 4] = value.to_le_bytes();
    let mut result: [u8; 8] = [0; 8];
    result[..4].copy_from_slice(&bytes);
    result
}

const fn bgum_use_method_to_token_metadata_use_method(
    bubblegum_use_method: BubblegumUseMethod,
) -> TokenMetadataUseMethod {
    match bubblegum_use_method {
        BubblegumUseMethod::Burn => TokenMetadataUseMethod::Burn,
        BubblegumUseMethod::Multiple => TokenMetadataUseMethod::Multiple,
        BubblegumUseMethod::Single => TokenMetadataUseMethod::Single,
    }
}
