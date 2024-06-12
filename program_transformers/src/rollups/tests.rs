use crate::error::RollupValidationError;
use crate::rollups::rollup_persister::{
    validate_rollup, ChangeLogEventV1, PathNode, RolledMintInstruction, Rollup,
};
use anchor_lang::AnchorSerialize;
use mpl_bubblegum::types::{LeafSchema, MetadataArgs};
use rand::{thread_rng, Rng};
use solana_sdk::keccak;
use solana_sdk::pubkey::Pubkey;
use spl_concurrent_merkle_tree::concurrent_merkle_tree::ConcurrentMerkleTree;
use std::collections::HashMap;
use std::str::FromStr;

fn generate_rollup(size: usize) -> Rollup {
    let authority = Pubkey::from_str("3VvLDXqJbw3heyRwFxv8MmurPznmDVUJS9gPMX2BDqfM").unwrap();
    let tree = Pubkey::from_str("HxhCw9g3kZvrdg9zZvctmh6qpSDg1FfsBXfFvRkbCHB7").unwrap();
    let mut mints = Vec::new();
    let mut merkle = ConcurrentMerkleTree::<10, 32>::new();
    merkle.initialize().unwrap();

    let mut last_leaf_hash = [0u8; 32];
    for i in 0..size {
        let mint_args = MetadataArgs {
            name: thread_rng()
                .sample_iter(rand::distributions::Alphanumeric)
                .take(15)
                .map(char::from)
                .collect(),
            symbol: thread_rng()
                .sample_iter(rand::distributions::Alphanumeric)
                .take(5)
                .map(char::from)
                .collect(),
            uri: format!(
                "https://arweave.net/{}",
                thread_rng()
                    .sample_iter(rand::distributions::Alphanumeric)
                    .take(43)
                    .map(char::from)
                    .collect::<String>()
            ),
            seller_fee_basis_points: thread_rng()
                .sample(rand::distributions::Uniform::new(0, 10000)),
            primary_sale_happened: thread_rng().gen_bool(0.5),
            is_mutable: thread_rng().gen_bool(0.5),
            edition_nonce: if thread_rng().gen_bool(0.5) {
                None
            } else {
                Some(thread_rng().sample(rand::distributions::Uniform::new(0, 255)))
            },
            token_standard: if thread_rng().gen_bool(0.5) {
                None
            } else {
                Some(mpl_bubblegum::types::TokenStandard::NonFungible)
            },
            collection: if thread_rng().gen_bool(0.5) {
                None
            } else {
                Some(mpl_bubblegum::types::Collection {
                    verified: false,
                    key: Pubkey::new_unique(),
                })
            },
            uses: None, // todo
            token_program_version: mpl_bubblegum::types::TokenProgramVersion::Original,
            creators: (0..thread_rng().sample(rand::distributions::Uniform::new(1, 5)))
                .map(|_| mpl_bubblegum::types::Creator {
                    address: Pubkey::new_unique(),
                    verified: false,
                    share: thread_rng().sample(rand::distributions::Uniform::new(0, 100)),
                })
                .collect(),
        };
        let nonce = i as u64;
        let id = mpl_bubblegum::utils::get_asset_id(&tree, nonce);
        let owner = authority.clone();
        let delegate = authority.clone();

        let metadata_args_hash = keccak::hashv(&[mint_args.try_to_vec().unwrap().as_slice()]);
        let data_hash = keccak::hashv(&[
            &metadata_args_hash.to_bytes(),
            &mint_args.seller_fee_basis_points.to_le_bytes(),
        ]);
        let creator_data = mint_args
            .creators
            .iter()
            .map(|c| [c.address.as_ref(), &[c.verified as u8], &[c.share]].concat())
            .collect::<Vec<_>>();
        let creator_hash = keccak::hashv(
            creator_data
                .iter()
                .map(|c| c.as_slice())
                .collect::<Vec<&[u8]>>()
                .as_ref(),
        );

        let hashed_leaf = keccak::hashv(&[
            &[1], //self.version().to_bytes()
            id.as_ref(),
            owner.as_ref(),
            delegate.as_ref(),
            nonce.to_le_bytes().as_ref(),
            data_hash.as_ref(),
            creator_hash.as_ref(),
        ])
        .to_bytes();
        merkle.append(hashed_leaf).unwrap();
        last_leaf_hash = hashed_leaf;
        let changelog = merkle.change_logs[merkle.active_index as usize];
        let path_len = changelog.path.len() as u32;
        let mut path: Vec<spl_account_compression::state::PathNode> = changelog
            .path
            .iter()
            .enumerate()
            .map(|(lvl, n)| {
                spl_account_compression::state::PathNode::new(
                    *n,
                    (1 << (path_len - lvl as u32)) + (changelog.index >> lvl),
                )
            })
            .collect();
        path.push(spl_account_compression::state::PathNode::new(
            changelog.root,
            1,
        ));

        let rolled_mint = RolledMintInstruction {
            tree_update: ChangeLogEventV1 {
                id: tree,
                path: path.into_iter().map(Into::into).collect::<Vec<_>>(),
                seq: merkle.sequence_number,
                index: changelog.index,
            },
            leaf_update: LeafSchema::V1 {
                id,
                owner,
                delegate,
                nonce,
                data_hash: data_hash.to_bytes(),
                creator_hash: creator_hash.to_bytes(),
            },
            mint_args,
            authority,
        };
        mints.push(rolled_mint);
    }
    let rollup = Rollup {
        tree_id: tree,
        raw_metadata_map: HashMap::new(),
        max_depth: 10,
        rolled_mints: mints,
        merkle_root: merkle.get_root(),
        last_leaf_hash,
        max_buffer_size: 32,
    };

    rollup
}

#[tokio::test]
async fn rollup_validation_test() {
    let mut rollup = generate_rollup(1000);

    let validation_result = validate_rollup(&rollup).await;
    assert_eq!(validation_result, Ok(()));

    let old_root = rollup.merkle_root;
    let new_root = Pubkey::new_unique();
    rollup.merkle_root = new_root.to_bytes();

    let validation_result = validate_rollup(&rollup).await;
    assert_eq!(
        validation_result,
        Err(RollupValidationError::InvalidRoot(
            Pubkey::from(old_root).to_string(),
            new_root.to_string()
        ))
    );

    rollup.merkle_root = old_root;
    let leaf_idx = 111;
    let old_leaf_data_hash = rollup.rolled_mints[leaf_idx].leaf_update.data_hash();
    let new_leaf_data_hash = Pubkey::new_unique();
    rollup.rolled_mints[leaf_idx].leaf_update = LeafSchema::V1 {
        id: rollup.rolled_mints[leaf_idx].leaf_update.id(),
        owner: rollup.rolled_mints[leaf_idx].leaf_update.owner(),
        delegate: rollup.rolled_mints[leaf_idx].leaf_update.delegate(),
        nonce: rollup.rolled_mints[leaf_idx].leaf_update.nonce(),
        data_hash: new_leaf_data_hash.to_bytes(),
        creator_hash: rollup.rolled_mints[leaf_idx].leaf_update.creator_hash(),
    };
    let validation_result = validate_rollup(&rollup).await;

    assert_eq!(
        validation_result,
        Err(RollupValidationError::InvalidDataHash(
            Pubkey::from(old_leaf_data_hash).to_string(),
            new_leaf_data_hash.to_string()
        ))
    );

    rollup.rolled_mints[leaf_idx].leaf_update = LeafSchema::V1 {
        id: rollup.rolled_mints[leaf_idx].leaf_update.id(),
        owner: rollup.rolled_mints[leaf_idx].leaf_update.owner(),
        delegate: rollup.rolled_mints[leaf_idx].leaf_update.delegate(),
        nonce: rollup.rolled_mints[leaf_idx].leaf_update.nonce(),
        data_hash: old_leaf_data_hash,
        creator_hash: rollup.rolled_mints[leaf_idx].leaf_update.creator_hash(),
    };
    let old_tree_depth = rollup.max_depth;
    let new_tree_depth = 100;
    rollup.max_depth = new_tree_depth;
    let validation_result = validate_rollup(&rollup).await;

    assert_eq!(
        validation_result,
        Err(RollupValidationError::UnexpectedTreeSize(
            new_tree_depth,
            rollup.max_buffer_size
        ))
    );

    rollup.max_depth = old_tree_depth;
    let new_asset_id = Pubkey::new_unique();
    let old_asset_id = rollup.rolled_mints[leaf_idx].leaf_update.id();
    rollup.rolled_mints[leaf_idx].leaf_update = LeafSchema::V1 {
        id: new_asset_id,
        owner: rollup.rolled_mints[leaf_idx].leaf_update.owner(),
        delegate: rollup.rolled_mints[leaf_idx].leaf_update.delegate(),
        nonce: rollup.rolled_mints[leaf_idx].leaf_update.nonce(),
        data_hash: rollup.rolled_mints[leaf_idx].leaf_update.data_hash(),
        creator_hash: rollup.rolled_mints[leaf_idx].leaf_update.creator_hash(),
    };
    let validation_result = validate_rollup(&rollup).await;

    assert_eq!(
        validation_result,
        Err(RollupValidationError::PDACheckFail(
            old_asset_id.to_string(),
            new_asset_id.to_string()
        ))
    );

    rollup.rolled_mints[leaf_idx].leaf_update = LeafSchema::V1 {
        id: old_asset_id,
        owner: rollup.rolled_mints[leaf_idx].leaf_update.owner(),
        delegate: rollup.rolled_mints[leaf_idx].leaf_update.delegate(),
        nonce: rollup.rolled_mints[leaf_idx].leaf_update.nonce(),
        data_hash: rollup.rolled_mints[leaf_idx].leaf_update.data_hash(),
        creator_hash: rollup.rolled_mints[leaf_idx].leaf_update.creator_hash(),
    };
    let old_path = rollup.rolled_mints[leaf_idx]
        .tree_update
        .path
        .iter()
        .map(|path| PathNode {
            node: path.node,
            index: path.index,
        })
        .collect::<Vec<_>>();
    let new_path = Vec::new();
    rollup.rolled_mints[leaf_idx].tree_update.path = new_path;
    let validation_result = validate_rollup(&rollup).await;

    assert_eq!(
        validation_result,
        Err(RollupValidationError::WrongAssetPath(
            rollup.rolled_mints[leaf_idx].leaf_update.id().to_string()
        ))
    );

    rollup.rolled_mints[leaf_idx].tree_update.path = old_path;
    let old_tree_id = rollup.rolled_mints[leaf_idx].tree_update.id;
    let new_tree_id = Pubkey::new_unique();
    rollup.rolled_mints[leaf_idx].tree_update.id = new_tree_id;
    let validation_result = validate_rollup(&rollup).await;

    assert_eq!(
        validation_result,
        Err(RollupValidationError::WrongTreeIdForChangeLog(
            rollup.rolled_mints[leaf_idx].leaf_update.id().to_string(),
            old_tree_id.to_string(),
            new_tree_id.to_string()
        ))
    );

    rollup.rolled_mints[leaf_idx].tree_update.id = old_tree_id;
    let old_index = rollup.rolled_mints[leaf_idx].tree_update.index;
    let new_index = 1;
    rollup.rolled_mints[leaf_idx].tree_update.index = new_index;
    let validation_result = validate_rollup(&rollup).await;

    assert_eq!(
        validation_result,
        Err(RollupValidationError::WrongChangeLogIndex(
            rollup.rolled_mints[leaf_idx].leaf_update.id().to_string(),
            old_index,
            new_index
        ))
    );
}
