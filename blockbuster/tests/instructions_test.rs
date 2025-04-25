#[cfg(test)]
mod helpers;
use anchor_lang::AnchorDeserialize;
use blockbuster::{
    instruction::{order_instructions, InstructionBundle},
    program_handler::ProgramParser,
    programs::{
        bubblegum::{BubblegumParser, LeafSchemaEvent, Payload},
        ProgramParseResult,
    },
};
use flatbuffers::FlatBufferBuilder;
use helpers::*;
use plerkle_serialization::root_as_transaction_info;
use rand::prelude::IteratorRandom;
use spl_account_compression::events::{
    AccountCompressionEvent::{self},
    ApplicationDataEvent, ApplicationDataEventV1, ChangeLogEvent, ChangeLogEventV1,
};
use std::{collections::HashSet, env};

#[test]
fn test_filter() {
    let mut rng = rand::thread_rng();
    let fbb = FlatBufferBuilder::new();
    let fbb = build_random_transaction(fbb);
    let data = fbb.finished_data();
    let txn = root_as_transaction_info(data).expect("TODO: panic message");
    let programs = get_programs(txn);
    let hs = programs
        .iter()
        .choose_multiple(&mut rng, 3)
        .into_iter()
        .copied()
        .collect::<HashSet<_>>();
    let (account_keys, message_instructions, meta_inner_instructions) = parse_fb(&txn);
    let res = order_instructions(
        &hs,
        &account_keys,
        &message_instructions,
        &meta_inner_instructions,
    );

    for (ib, _inner) in res.iter() {
        let public_key_matches = hs.contains(&ib.0);
        assert!(public_key_matches);
    }

    let res = order_instructions(
        &HashSet::new(),
        &account_keys,
        &message_instructions,
        &meta_inner_instructions,
    );
    assert_eq!(res.len(), 0);
}

fn prepare_fixture<'a>(fbb: FlatBufferBuilder<'a>, fixture: &'a str) -> FlatBufferBuilder<'a> {
    println!("{:?}", env::current_dir());
    let name = fixture.to_string();
    let fbb = build_txn_from_fixture(name, fbb).unwrap();
    fbb
}

#[test]
fn helium_nested() {
    let fbb = FlatBufferBuilder::new();
    let txn = prepare_fixture(fbb, "helium_nested");
    let txn = root_as_transaction_info(txn.finished_data()).expect("Fail deser");
    let mut prog = HashSet::new();
    let id = mpl_bubblegum::ID;
    let slot = txn.slot();
    prog.insert(id);
    let (account_keys, message_instructions, meta_inner_instructions) = parse_fb(&txn);
    let res = order_instructions(
        &prog,
        &account_keys,
        &message_instructions,
        &meta_inner_instructions,
    );

    let _ix = 0;

    let contains = res.iter().any(|(ib, _inner)| ib.0 == mpl_bubblegum::ID);
    assert!(contains, "Must containe bgum at hoisted root");
    let subject = BubblegumParser {};
    for (outer_ix, inner_ix) in res.into_iter() {
        let (program, instruction) = outer_ix;
        // let ix_accounts = instruction.accounts.iter().collect::<Vec<_>>();
        let ix_account_len = instruction.accounts.len();
        // let _max = ix_accounts.iter().max().copied().unwrap_or(0) as usize;
        let ix_accounts =
            instruction
                .accounts
                .iter()
                .fold(Vec::with_capacity(ix_account_len), |mut acc, a| {
                    if let Some(key) = account_keys.get(*a as usize) {
                        acc.push(*key);
                    }
                    //else case here is handled on 272
                    acc
                });
        let bundle = InstructionBundle {
            txn_id: "",
            program,
            instruction: Some(instruction),
            inner_ix: inner_ix.as_deref(),
            keys: ix_accounts.as_slice(),
            slot,
        };
        let result = subject.handle_instruction(&bundle).unwrap();
        let res_type = result.result_type();
        let parse_result = match res_type {
            ProgramParseResult::Bubblegum(parse_result) => parse_result,
            _ => panic!("Wrong type"),
        };

        if let (
            Some(_le),
            Some(_cl),
            Some(Payload::Mint {
                args: _,
                authority: _,
                tree_id: _,
            }),
        ) = (
            &parse_result.leaf_update,
            &parse_result.tree_update,
            &parse_result.payload,
        ) {
        } else {
            panic!("Failed to parse instruction");
        }
    }
}

#[test]
fn test_double_mint() {
    let fbb = FlatBufferBuilder::new();
    let txn = prepare_fixture(fbb, "double_bubblegum_mint");
    let txn = root_as_transaction_info(txn.finished_data()).expect("Fail deser");
    let mut programs = HashSet::new();
    let subject = BubblegumParser {}.key();
    programs.insert(subject);
    let (account_keys, message_instructions, meta_inner_instructions) = parse_fb(&txn);
    let ix = order_instructions(
        &programs,
        &account_keys,
        &message_instructions,
        &meta_inner_instructions,
    );
    assert_eq!(ix.len(), 2);
    let contains = ix.iter().filter(|(ib, _inner)| ib.0 == mpl_bubblegum::ID);
    let mut count = 0;
    contains.for_each(|(_pk, cix)| {
        count += 1;
        if let Some(inner) = &cix {
            println!("{}", inner.len());
            for ii in inner {
                println!("pp{} {:?}", count, ii.0);
            }
            println!("------");
            let cl = AccountCompressionEvent::try_from_slice(&inner[1].1.data).unwrap();
            if let AccountCompressionEvent::ApplicationData(ApplicationDataEvent::V1(
                ApplicationDataEventV1 { application_data },
            )) = cl
            {
                let lse = LeafSchemaEvent::try_from_slice(&application_data).unwrap();
                println!("1 pp{} NONCE {:?}\n end", count, lse.schema.nonce());
            }
            let cl = AccountCompressionEvent::try_from_slice(&inner[3].1.data).unwrap();
            if let AccountCompressionEvent::ChangeLog(ChangeLogEvent::V1(ChangeLogEventV1 {
                id,
                ..
            })) = cl
            {
                println!("2 pp{} Merkle Tree {:?} \n end", count, id);
            }
        }
    });
    assert_eq!(count, 2);
}

#[test]
fn test_double_tree() {
    let fbb = FlatBufferBuilder::new();
    let txn = prepare_fixture(fbb, "helium_mint_double_tree");
    let txn = root_as_transaction_info(txn.finished_data()).expect("Fail deser");
    let mut programs = HashSet::new();
    let subject = BubblegumParser {}.key();
    programs.insert(subject);
    let (account_keys, message_instructions, meta_inner_instructions) = parse_fb(&txn);
    let ix = order_instructions(
        &programs,
        &account_keys,
        &message_instructions,
        &meta_inner_instructions,
    );
    let contains = ix.iter().filter(|(ib, _inner)| ib.0 == mpl_bubblegum::ID);
    let mut count = 0;
    contains.for_each(|(_pk, cix)| {
        if let Some(inner) = &cix {
            for ii in inner {
                println!("pp{} {:?}", count, ii.0);
            }
            println!("------");
            let cl = AccountCompressionEvent::try_from_slice(&inner[1].1.data).unwrap();
            if let AccountCompressionEvent::ApplicationData(ApplicationDataEvent::V1(
                ApplicationDataEventV1 { application_data },
            )) = cl
            {
                let lse = LeafSchemaEvent::try_from_slice(&application_data).unwrap();
                println!("1 pp{} NONCE {:?}\n end", count, lse.schema.nonce());
            }
            let cl = AccountCompressionEvent::try_from_slice(&inner[3].1.data).unwrap();
            if let AccountCompressionEvent::ChangeLog(ChangeLogEvent::V1(ChangeLogEventV1 {
                id,
                ..
            })) = cl
            {
                println!("2 pp{} Merkle Tree {:?} \n end", count, id);
            }
        }
        count += 1;
    });
    assert_eq!(count, 2);
}
