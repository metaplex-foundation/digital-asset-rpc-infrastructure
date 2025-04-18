use {
    crate::{
        error::{ProgramTransformerError, ProgramTransformerResult},
        DownloadMetadataNotifier,
    },
    blockbuster::{
        instruction::InstructionBundle,
        programs::bubblegum::{
            BubblegumInstruction, InstructionName, LeafSchema, UseMethod as BubblegumUseMethod,
        },
        token_metadata::types::UseMethod as TokenMetadataUseMethod,
    },
    sea_orm::{ConnectionTrait, TransactionTrait},
    solana_sdk::pubkey::Pubkey,
    tracing::debug,
};

mod burn;
mod cancel_redeem;
mod collection_verification;
mod creator_verification;
mod db;
mod delegate;
mod mint;
mod redeem;
mod transfer;
mod update_metadata;

pub async fn handle_bubblegum_instruction<'c, T>(
    parsing_result: &'c BubblegumInstruction,
    bundle: &'c InstructionBundle<'c>,
    txn: &T,
    download_metadata_notifier: &DownloadMetadataNotifier,
) -> ProgramTransformerResult<()>
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
        InstructionName::BurnV2 => "BurnV2",
        InstructionName::CreateTreeV2 => "CreateTreeV2",
        InstructionName::DelegateAndFreezeV2 => "DelegateAndFreezeV2",
        InstructionName::DelegateV2 => "DelegateV2",
        InstructionName::FreezeV2 => "FreezeV2",
        InstructionName::MintV2 => "MintV2",
        InstructionName::SetCollectionV2 => "SetCollectionV2",
        InstructionName::SetNonTransferableV2 => "SetNonTransferableV2",
        InstructionName::ThawAndRevokeV2 => "ThawAndRevokeV2",
        InstructionName::ThawV2 => "ThawV2",
        InstructionName::TransferV2 => "TransferV2",
        InstructionName::UnverifyCreatorV2 => "UnverifyCreatorV2",
        InstructionName::UpdateAssetDataV2 => "UpdateAssetDataV2",
        InstructionName::UpdateMetadataV2 => "UpdateMetadataV2",
        InstructionName::VerifyCreatorV2 => "VerifyCreatorV2",
    };
    debug!("BGUM instruction txn={:?}: {:?}", ix_str, bundle.txn_id);

    match ix_type {
        InstructionName::Transfer | InstructionName::TransferV2 => {
            transfer::transfer(parsing_result, bundle, txn, ix_str).await?;
        }
        InstructionName::Burn | InstructionName::BurnV2 => {
            burn::burn(parsing_result, bundle, txn, ix_str).await?;
        }
        InstructionName::Delegate
        | InstructionName::DelegateV2
        | InstructionName::DelegateAndFreezeV2
        | InstructionName::FreezeV2
        | InstructionName::SetNonTransferableV2
        | InstructionName::ThawV2
        | InstructionName::ThawAndRevokeV2 => {
            delegate::delegation_freezing_nontransferability(parsing_result, bundle, txn, ix_str)
                .await?;
        }
        InstructionName::MintV1 | InstructionName::MintToCollectionV1 | InstructionName::MintV2 => {
            if let Some(info) = mint::mint(parsing_result, bundle, txn, ix_str).await? {
                download_metadata_notifier(info)
                    .await
                    .map_err(ProgramTransformerError::DownloadMetadataNotify)?;
            }
        }
        InstructionName::Redeem => {
            redeem::redeem(parsing_result, bundle, txn, ix_str).await?;
        }
        InstructionName::CancelRedeem => {
            cancel_redeem::cancel_redeem(parsing_result, bundle, txn, ix_str).await?;
        }
        InstructionName::DecompressV1 => {
            debug!("No action necessary for decompression")
        }
        InstructionName::VerifyCreator
        | InstructionName::UnverifyCreator
        | InstructionName::VerifyCreatorV2
        | InstructionName::UnverifyCreatorV2 => {
            creator_verification::process(parsing_result, bundle, txn, ix_str).await?;
        }
        InstructionName::VerifyCollection
        | InstructionName::UnverifyCollection
        | InstructionName::SetAndVerifyCollection
        | InstructionName::SetCollectionV2 => {
            collection_verification::process(parsing_result, bundle, txn, ix_str).await?;
        }
        InstructionName::SetDecompressibleState => (), // Nothing to index.
        InstructionName::UpdateMetadata | InstructionName::UpdateMetadataV2 => {
            if let Some(info) =
                update_metadata::update_metadata(parsing_result, bundle, txn, ix_str).await?
            {
                download_metadata_notifier(info)
                    .await
                    .map_err(ProgramTransformerError::DownloadMetadataNotify)?;
            }
        }
        InstructionName::UpdateAssetDataV2 => debug!("Bubblegum: Not Implemented Instruction"),
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

/// A normalized representation of both V1 and V2 leaf schemas,
/// providing a unified view of all fields.
///
/// Fields that are only present in V2 (i.e. `collection_hash`, `asset_data_hash`, `flags`)
/// are represented as `Option`s and will be `None` when derived from a LeafSchema V1 struct.
pub(crate) struct NormalizedLeafFields {
    id: Pubkey,
    owner: Pubkey,
    delegate: Pubkey,
    nonce: u64,
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    collection_hash: Option<[u8; 32]>,
    asset_data_hash: Option<[u8; 32]>,
    flags: Option<u8>,
}

impl From<&LeafSchema> for NormalizedLeafFields {
    fn from(leaf_schema: &LeafSchema) -> Self {
        match leaf_schema {
            LeafSchema::V1 {
                id,
                owner,
                delegate,
                nonce,
                data_hash,
                creator_hash,
                ..
            } => Self {
                id: *id,
                owner: *owner,
                delegate: *delegate,
                nonce: *nonce,
                data_hash: *data_hash,
                creator_hash: *creator_hash,
                collection_hash: None,
                asset_data_hash: None,
                flags: None,
            },
            LeafSchema::V2 {
                id,
                owner,
                delegate,
                nonce,
                data_hash,
                creator_hash,
                collection_hash,
                asset_data_hash,
                flags,
                ..
            } => Self {
                id: *id,
                owner: *owner,
                delegate: *delegate,
                nonce: *nonce,
                data_hash: *data_hash,
                creator_hash: *creator_hash,
                collection_hash: Some(*collection_hash),
                asset_data_hash: Some(*asset_data_hash),
                flags: Some(*flags),
            },
        }
    }
}
