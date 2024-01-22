use anyhow::Result;
use borsh::BorshDeserialize;
use clap::Args;
use sea_orm::{DatabaseConnection, DbBackend, FromQueryResult, Statement, Value};
use solana_client::rpc_filter::{Memcmp, RpcFilterType};
use solana_sdk::{account::Account, pubkey::Pubkey, signature::Signature};
use spl_account_compression::id;
use spl_account_compression::state::{
    merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
};
use std::str::FromStr;
use thiserror::Error as ThisError;
use tokio::sync::mpsc::Sender;

use crate::{queue::QueuePoolError, rpc::Rpc};

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
    #[error("sea orm")]
    Database(#[from] sea_orm::DbErr),
    #[error("try from pubkey")]
    TryFromPubkey,
    #[error("try from signature")]
    TryFromSignature,
}

const TREE_GAP_SQL: &str = r#"
WITH sequenced_data AS (
    SELECT
        tree,
        seq,
        LEAD(seq) OVER (ORDER BY seq ASC) AS next_seq,
        tx AS current_tx,
        LEAD(tx) OVER (ORDER BY seq ASC) AS next_tx
    FROM
        cl_audits_v2
    WHERE
        tree = $1
),
gaps AS (
    SELECT
        tree,
        seq AS gap_start_seq,
        next_seq AS gap_end_seq,
        current_tx AS lower_bound_tx,
        next_tx AS upper_bound_tx
    FROM
        sequenced_data
    WHERE
        next_seq IS NOT NULL AND
        next_seq - seq > 1
)
SELECT
    tree,
    gap_start_seq,
    gap_end_seq,
    lower_bound_tx,
    upper_bound_tx
FROM
    gaps
ORDER BY
    gap_start_seq;
"#;

#[derive(Debug, FromQueryResult, PartialEq, Clone)]
pub struct TreeGapModel {
    pub tree: Vec<u8>,
    pub gap_start_seq: i64,
    pub gap_end_seq: i64,
    pub lower_bound_tx: Vec<u8>,
    pub upper_bound_tx: Vec<u8>,
}

impl TreeGapModel {
    pub async fn find(conn: &DatabaseConnection, tree: Pubkey) -> Result<Vec<Self>, TreeErrorKind> {
        let statement = Statement::from_sql_and_values(
            DbBackend::Postgres,
            TREE_GAP_SQL,
            vec![Value::Bytes(Some(Box::new(tree.as_ref().to_vec())))],
        );

        TreeGapModel::find_by_statement(statement)
            .all(conn)
            .await
            .map_err(Into::into)
    }
}

impl TryFrom<TreeGapModel> for TreeGapFill {
    type Error = TreeErrorKind;

    fn try_from(model: TreeGapModel) -> Result<Self, Self::Error> {
        let tree = Pubkey::try_from(model.tree).map_err(|_| TreeErrorKind::TryFromPubkey)?;
        let upper = Signature::try_from(model.upper_bound_tx)
            .map_err(|_| TreeErrorKind::TryFromSignature)?;
        let lower = Signature::try_from(model.lower_bound_tx)
            .map_err(|_| TreeErrorKind::TryFromSignature)?;

        Ok(Self::new(tree, Some(upper), Some(lower)))
    }
}

pub struct TreeGapFill {
    tree: Pubkey,
    before: Option<Signature>,
    until: Option<Signature>,
}

impl TreeGapFill {
    pub fn new(tree: Pubkey, before: Option<Signature>, until: Option<Signature>) -> Self {
        Self {
            tree,
            before,
            until,
        }
    }

    pub async fn crawl(&self, client: &Rpc, sender: Sender<Signature>) -> Result<()> {
        let mut before = self.before;

        loop {
            let sigs = client
                .get_signatures_for_address(&self.tree, before, self.until)
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
    pub seq: u64,
}

impl TreeResponse {
    pub fn try_from_rpc(pubkey: Pubkey, account: Account) -> Result<Self> {
        let bytes = account.data.as_slice();

        let (header_bytes, rest) = bytes.split_at(CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1);
        let header: ConcurrentMerkleTreeHeader =
            ConcurrentMerkleTreeHeader::try_from_slice(header_bytes)?;

        let merkle_tree_size = merkle_tree_get_size(&header)?;
        let (tree_bytes, _canopy_bytes) = rest.split_at(merkle_tree_size);

        let seq_bytes = tree_bytes[0..8].try_into()?;
        let seq = u64::from_le_bytes(seq_bytes);

        let (auth, _) = Pubkey::find_program_address(&[pubkey.as_ref()], &mpl_bubblegum::ID);

        header.assert_valid_authority(&auth)?;

        let tree_header = header.try_into()?;

        Ok(Self {
            pubkey,
            tree_header,
            seq,
        })
    }

    pub async fn all(client: &Rpc) -> Result<Vec<Self>, TreeErrorKind> {
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
            .filter_map(|(pubkey, account)| Self::try_from_rpc(pubkey, account).ok())
            .collect())
    }

    pub async fn find(client: &Rpc, pubkeys: Vec<String>) -> Result<Vec<Self>, TreeErrorKind> {
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
                    batch.iter().zip(accounts).collect();

                Ok::<_, TreeErrorKind>(results)
            })
        }

        let result = futures::future::try_join_all(gma_handles).await?;

        let trees = result
            .into_iter()
            .flatten()
            .filter_map(|(pubkey, account)| {
                account.map(|account| Self::try_from_rpc(*pubkey, account))
            })
            .collect::<Result<Vec<TreeResponse>, _>>()
            .map_err(|_| TreeErrorKind::SerializeTreeResponse)?;

        Ok(trees)
    }
}
