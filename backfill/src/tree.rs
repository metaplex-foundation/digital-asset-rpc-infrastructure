use super::ErrorKind;
use anyhow::Result;
use borsh::BorshDeserialize;
use das_core::Rpc;
use solana_client::rpc_filter::{Memcmp, RpcFilterType};
use solana_sdk::{account::Account, pubkey::Pubkey};
use spl_account_compression::id;
use spl_account_compression::state::{
    merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct TreeHeaderResponse {
    pub max_depth: u32,
    pub max_buffer_size: u32,
    pub creation_slot: u64,
    pub size: usize,
}

impl TryFrom<ConcurrentMerkleTreeHeader> for TreeHeaderResponse {
    type Error = ErrorKind;

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

    pub async fn all(client: &Rpc) -> Result<Vec<Self>, ErrorKind> {
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

    pub async fn find(client: &Rpc, pubkeys: Vec<String>) -> Result<Vec<Self>, ErrorKind> {
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

                let results: Vec<(&Pubkey, Option<Account>)> = batch.iter().zip(accounts).collect();

                Ok::<_, ErrorKind>(results)
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
            .map_err(|_| ErrorKind::SerializeTreeResponse)?;

        Ok(trees)
    }
}
