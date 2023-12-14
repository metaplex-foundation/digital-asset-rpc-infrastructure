use anyhow::Result;
use borsh::BorshDeserialize;
use clap::Args;
use digital_asset_types::dao::tree_transactions;
use flatbuffers::FlatBufferBuilder;
use log::info;
use plerkle_serialization::serializer::seralize_encoded_transaction_with_status;
use sea_orm::{
    sea_query::OnConflict, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    QueryFilter, QueryOrder,
};
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::{
    account::Account,
    commitment_config::{CommitmentConfig, CommitmentLevel},
    pubkey::Pubkey,
    signature::Signature,
};
use solana_transaction_status::UiTransactionEncoding;
use spl_account_compression::id;
use spl_account_compression::state::{
    merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
};
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error as ThisError;
use tokio::sync::mpsc::Sender;

const GET_SIGNATURES_FOR_ADDRESS_LIMIT: usize = 1000;

#[derive(Debug, Clone, Args)]
pub struct ConfigBackfiller {
    /// Solana RPC URL
    #[arg(long, env)]
    pub solana_rpc_url: String,
}

#[derive(ThisError, Debug)]
pub enum TreeErrorKind {
    #[error("solana rpc")]
    Rpc(#[from] solana_client::client_error::ClientError),
    #[error("anchor")]
    Achor(#[from] anchor_client::anchor_lang::error::Error),
    #[error("perkle serialize")]
    PerkleSerialize(#[from] plerkle_serialization::error::PlerkleSerializationError),
    #[error("perkle messenger")]
    PlerkleMessenger(#[from] plerkle_messenger::MessengerError),
    #[error("queue send")]
    QueueSend(#[from] tokio::sync::mpsc::error::SendError<Vec<u8>>),
}
#[derive(Debug, Clone)]
pub struct TreeHeaderResponse {
    pub max_depth: u32,
    pub max_buffer_size: u32,
    pub creation_slot: u64,
    pub size: usize,
}

impl TryFrom<ConcurrentMerkleTreeHeader> for TreeHeaderResponse {
    type Error = TreeErrorKind;

    fn try_from(payload: ConcurrentMerkleTreeHeader) -> Result<Self, Self::Error> {
        let size = merkle_tree_get_size(&payload)?;
        Ok(Self {
            max_depth: payload.get_max_depth(),
            max_buffer_size: payload.get_max_buffer_size(),
            creation_slot: payload.get_creation_slot(),
            size,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TreeResponse {
    pub pubkey: Pubkey,
    pub tree_header: TreeHeaderResponse,
}

impl TreeResponse {
    pub fn try_from_rpc(pubkey: Pubkey, account: Account) -> Result<Self> {
        let (header_bytes, _rest) = account.data.split_at(CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1);
        let header: ConcurrentMerkleTreeHeader =
            ConcurrentMerkleTreeHeader::try_from_slice(header_bytes)?;

        let (auth, _) = Pubkey::find_program_address(&[pubkey.as_ref()], &mpl_bubblegum::ID);

        header.assert_valid_authority(&auth)?;

        let tree_header = header.try_into()?;

        Ok(Self {
            pubkey,
            tree_header,
        })
    }
    pub async fn crawl(
        &self,
        client: Arc<RpcClient>,
        sender: Sender<Signature>,
        conn: DatabaseConnection,
    ) -> Result<()> {
        let mut before = None;

        let until = tree_transactions::Entity::find()
            .filter(tree_transactions::Column::Tree.eq(self.pubkey.as_ref()))
            .order_by_desc(tree_transactions::Column::Slot)
            .one(&conn)
            .await?
            .and_then(|t| Signature::from_str(&t.signature).ok());

        loop {
            let sigs = client
                .get_signatures_for_address_with_config(
                    &self.pubkey,
                    GetConfirmedSignaturesForAddress2Config {
                        before,
                        until,
                        commitment: Some(CommitmentConfig {
                            commitment: CommitmentLevel::Finalized,
                        }),
                        ..GetConfirmedSignaturesForAddress2Config::default()
                    },
                )
                .await?;

            for sig in sigs.iter() {
                let slot = i64::try_from(sig.slot)?;
                let sig = Signature::from_str(&sig.signature)?;

                let tree_transaction_processed = tree_transactions::Entity::find()
                    .filter(
                        tree_transactions::Column::Signature
                            .eq(sig.to_string())
                            .and(tree_transactions::Column::ProcessedAt.is_not_null()),
                    )
                    .one(&conn)
                    .await?;

                if tree_transaction_processed.is_some() {
                    info!("skipping previously processed transaction {}", sig);
                    continue;
                }

                let tree_transaction = tree_transactions::ActiveModel {
                    signature: Set(sig.to_string()),
                    tree: Set(self.pubkey.as_ref().to_vec()),
                    slot: Set(slot),
                    ..Default::default()
                };

                tree_transactions::Entity::insert(tree_transaction)
                    .on_conflict(
                        OnConflict::column(tree_transactions::Column::Signature)
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec(&conn)
                    .await?;

                sender.send(sig).await?;

                before = Some(sig);
            }

            if sigs.len() < GET_SIGNATURES_FOR_ADDRESS_LIMIT {
                break;
            }
        }

        Ok(())
    }
}

pub async fn all(client: &Arc<RpcClient>) -> Result<Vec<TreeResponse>, TreeErrorKind> {
    let config = RpcProgramAccountsConfig {
        filters: Some(vec![RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            0,
            vec![1u8],
        ))]),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig {
                commitment: CommitmentLevel::Finalized,
            }),
            ..RpcAccountInfoConfig::default()
        },
        ..RpcProgramAccountsConfig::default()
    };

    Ok(client
        .get_program_accounts_with_config(&id(), config)
        .await?
        .into_iter()
        .filter_map(|(pubkey, account)| TreeResponse::try_from_rpc(pubkey, account).ok())
        .collect())
}

pub async fn transaction<'a>(
    client: Arc<RpcClient>,
    sender: Sender<Vec<u8>>,
    signature: Signature,
) -> Result<(), TreeErrorKind> {
    let transaction = client
        .get_transaction_with_config(
            &signature,
            RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Base58),
                max_supported_transaction_version: Some(0),
                commitment: Some(CommitmentConfig {
                    commitment: CommitmentLevel::Finalized,
                }),
                ..RpcTransactionConfig::default()
            },
        )
        .await?;

    let message = seralize_encoded_transaction_with_status(FlatBufferBuilder::new(), transaction)?;

    sender.send(message.finished_data().to_vec()).await?;

    Ok(())
}
