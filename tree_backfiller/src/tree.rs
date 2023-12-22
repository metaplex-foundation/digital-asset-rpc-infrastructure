use anyhow::Result;
use borsh::BorshDeserialize;
use clap::Args;
use flatbuffers::FlatBufferBuilder;
use log::error;
use plerkle_serialization::serializer::seralize_encoded_transaction_with_status;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};
use solana_client::rpc_filter::{Memcmp, RpcFilterType};
use solana_sdk::{account::Account, pubkey::Pubkey, signature::Signature};
use spl_account_compression::id;
use spl_account_compression::state::{
    merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
};
use std::str::FromStr;
use thiserror::Error as ThisError;
use tokio::sync::mpsc::Sender;

use crate::backfiller::CrawlDirection;
use crate::{
    queue::{QueuePool, QueuePoolError},
    rpc::Rpc,
};

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
    #[error("queue pool")]
    QueuePool(#[from] QueuePoolError),
    #[error("parse pubkey")]
    ParsePubkey(#[from] solana_sdk::pubkey::ParsePubkeyError),
    #[error("serialize tree response")]
    SerializeTreeResponse,
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
    pub async fn crawl(&self, client: &Rpc, sender: Sender<Signature>) -> Result<()> {
        let mut before = None;

        loop {
            let sigs = client
                .get_signatures_for_address(&self.pubkey, before)
                .await?;

            for sig in sigs.iter() {
                let sig = Signature::from_str(&sig.signature)?;

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

pub async fn all(client: &Rpc) -> Result<Vec<TreeResponse>, TreeErrorKind> {
    Ok(client
        .get_program_accounts(
            &id(),
            Some(vec![RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                0,
                vec![1u8],
            ))]),
        )
        .await?
        .into_iter()
        .filter_map(|(pubkey, account)| TreeResponse::try_from_rpc(pubkey, account).ok())
        .collect())
}

pub async fn find(client: &Rpc, pubkeys: Vec<String>) -> Result<Vec<TreeResponse>, TreeErrorKind> {
    let pubkeys: Vec<Pubkey> = pubkeys
        .into_iter()
        .map(|p| Pubkey::from_str(&p))
        .collect::<Result<Vec<Pubkey>, _>>()?;
    let pubkey_batches = pubkeys.chunks(100);
    let pubkey_batches_count = pubkey_batches.len();

    let mut gma_handles = Vec::with_capacity(pubkey_batches_count);

    for batch in pubkey_batches {
        gma_handles.push(async move {
            let accounts = client.get_multiple_accounts(batch).await?;

            let results: Vec<(&Pubkey, Option<Account>)> =
                batch.into_iter().zip(accounts).collect();

            Ok::<_, TreeErrorKind>(results)
        })
    }

    let result = futures::future::try_join_all(gma_handles).await?;

    let trees = result
        .into_iter()
        .flatten()
        .filter_map(|(pubkey, account)| {
            if let Some(account) = account {
                Some(TreeResponse::try_from_rpc(*pubkey, account))
            } else {
                None
            }
        })
        .collect::<Result<Vec<TreeResponse>, _>>()
        .map_err(|_| TreeErrorKind::SerializeTreeResponse)?;

    Ok(trees)
}

pub async fn transaction<'a>(
    client: &Rpc,
    queue: QueuePool,
    signature: Signature,
) -> Result<(), TreeErrorKind> {
    let transaction = client.get_transaction(&signature).await?;

    let message = seralize_encoded_transaction_with_status(FlatBufferBuilder::new(), transaction)?;

    queue.push(message.finished_data()).await?;

    Ok(())
}
