#[cfg(test)]
use blockbuster::{
    program_handler::ProgramParser,
    programs::{bubblegum::BubblegumParser, ProgramParseResult},
};
use flatbuffers::FlatBufferBuilder;
use helpers::*;
use mpl_bubblegum::{
    instructions::{MintV1InstructionArgs, TransferInstructionArgs},
    types::{BubblegumEventType, Creator, LeafSchema, MetadataArgs, TokenProgramVersion, Version},
    LeafSchemaEvent,
};
use spl_account_compression::{
    events::{AccountCompressionEvent, ChangeLogEvent},
    state::PathNode,
};

mod helpers;

#[test]
fn test_setup() {
    let subject = BubblegumParser {};
    assert_eq!(subject.key(), mpl_bubblegum::ID);
    assert!(subject.key_match(&mpl_bubblegum::ID));
}

#[test]
fn test_mint() {
    let subject = BubblegumParser {};

    let accounts = random_list_of(9, |_i| random_pubkey());
    let fb_accounts = accounts.clone();
    let fb_account_indexes: Vec<u8> = fb_accounts
        .iter()
        .enumerate()
        .map(|(i, _)| i as u8)
        .collect();

    let metadata = MetadataArgs {
        name: "test".to_string(),
        symbol: "test".to_string(),
        uri: "www.solana.pos".to_owned(),
        seller_fee_basis_points: 0,
        primary_sale_happened: false,
        is_mutable: false,
        edition_nonce: None,
        token_standard: None,
        token_program_version: TokenProgramVersion::Original,
        collection: None,
        uses: None,
        creators: vec![Creator {
            address: random_pubkey(),
            verified: false,
            share: 20,
        }],
    };

    // We are only using this to get the instruction data, so the accounts don't actually matter
    // here.
    let mut accounts_iter = accounts.iter();
    let ix = mpl_bubblegum::instructions::MintV1 {
        tree_config: *accounts_iter.next().unwrap(),
        leaf_owner: *accounts_iter.next().unwrap(),
        leaf_delegate: *accounts_iter.next().unwrap(),
        merkle_tree: *accounts_iter.next().unwrap(),
        payer: *accounts_iter.next().unwrap(),
        tree_creator_or_delegate: *accounts_iter.next().unwrap(),
        log_wrapper: *accounts_iter.next().unwrap(),
        compression_program: *accounts_iter.next().unwrap(),
        system_program: *accounts_iter.next().unwrap(),
    };
    let ix_data = ix.instruction(MintV1InstructionArgs { metadata }).data;

    let lse = LeafSchemaEvent {
        event_type: BubblegumEventType::LeafSchemaEvent,
        version: Version::V1,
        schema: LeafSchema::V1 {
            id: random_pubkey(),
            owner: random_pubkey(),
            delegate: random_pubkey(),
            nonce: 0,
            data_hash: [0; 32],
            creator_hash: [0; 32],
        },
        leaf_hash: [0; 32],
    };

    let cs = ChangeLogEvent::new(
        random_pubkey(),
        vec![PathNode {
            node: [0; 32],
            index: 0,
        }],
        0,
        0,
    );
    let cs_event = AccountCompressionEvent::ChangeLog(cs);

    let mut fbb1 = FlatBufferBuilder::new();
    let mut fbb2 = FlatBufferBuilder::new();
    let mut fbb3 = FlatBufferBuilder::new();
    let mut fbb4 = FlatBufferBuilder::new();

    let ix_b = build_bubblegum_bundle(
        &mut fbb1,
        &mut fbb2,
        &mut fbb3,
        &mut fbb4,
        &fb_accounts,
        &fb_account_indexes,
        &ix_data,
        lse,
        cs_event,
    );

    let result = subject.handle_instruction(&ix_b);

    if let ProgramParseResult::Bubblegum(b) = result.unwrap().result_type() {
        let matched = match b.instruction {
            mpl_bubblegum::InstructionName::MintV1 => Ok(()),
            _ => Err(()),
        };
        assert!(matched.is_ok());
        assert!(b.payload.is_some());
        assert!(b.leaf_update.is_some());
        assert!(b.tree_update.is_some());
    } else {
        panic!("Unexpected ProgramParseResult variant");
    }
}

#[test]
fn test_basic_success_parsing() {
    let subject = BubblegumParser {};

    let accounts = random_list_of(8, |_i| random_pubkey());
    let fb_accounts = accounts.clone();
    let fb_account_indexes: Vec<u8> = fb_accounts
        .iter()
        .enumerate()
        .map(|(i, _)| i as u8)
        .collect();

    // We are only using this to get the instruction data, so the accounts don't actually matter
    // here.
    let mut accounts_iter = accounts.iter();
    let ix = mpl_bubblegum::instructions::Transfer {
        tree_config: *accounts_iter.next().unwrap(),
        leaf_owner: (*accounts_iter.next().unwrap(), true),
        leaf_delegate: (*accounts_iter.next().unwrap(), false),
        merkle_tree: *accounts_iter.next().unwrap(),
        log_wrapper: *accounts_iter.next().unwrap(),
        compression_program: *accounts_iter.next().unwrap(),
        system_program: *accounts_iter.next().unwrap(),
        new_leaf_owner: *accounts_iter.next().unwrap(),
    };
    let ix_data = ix
        .instruction(TransferInstructionArgs {
            root: [0; 32],
            data_hash: [0; 32],
            creator_hash: [0; 32],
            nonce: 0,
            index: 0,
        })
        .data;

    let lse = LeafSchemaEvent {
        event_type: BubblegumEventType::LeafSchemaEvent,
        version: Version::V1,
        schema: LeafSchema::V1 {
            id: random_pubkey(),
            owner: random_pubkey(),
            delegate: random_pubkey(),
            nonce: 0,
            data_hash: [0; 32],
            creator_hash: [0; 32],
        },
        leaf_hash: [0; 32],
    };

    let cs = ChangeLogEvent::new(
        random_pubkey(),
        vec![PathNode {
            node: [0; 32],
            index: 0,
        }],
        0,
        0,
    );
    let cs_event = AccountCompressionEvent::ChangeLog(cs);

    let mut fbb1 = FlatBufferBuilder::new();
    let mut fbb2 = FlatBufferBuilder::new();
    let mut fbb3 = FlatBufferBuilder::new();
    let mut fbb4 = FlatBufferBuilder::new();

    let ix_b = build_bubblegum_bundle(
        &mut fbb1,
        &mut fbb2,
        &mut fbb3,
        &mut fbb4,
        &fb_accounts,
        &fb_account_indexes,
        &ix_data,
        lse,
        cs_event,
    );
    let result = subject.handle_instruction(&ix_b);

    if let ProgramParseResult::Bubblegum(b) = result.unwrap().result_type() {
        assert!(b.payload.is_none());
        let matched = match b.instruction {
            mpl_bubblegum::InstructionName::Transfer => Ok(()),
            _ => Err(()),
        };
        assert!(matched.is_ok());
        assert!(b.leaf_update.is_some());
        assert!(b.tree_update.is_some());
    } else {
        panic!("Unexpected ProgramParseResult variant");
    }
}
