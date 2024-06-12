// Workaround since this module is only used for testing.
#![allow(dead_code)]
use blockbuster::{
    error::BlockbusterError,
    instruction::{InstructionBundle, IxPair},
};
use borsh::ser::BorshSerialize;
use flatbuffers::{FlatBufferBuilder, WIPOffset};
use mpl_bubblegum::LeafSchemaEvent;
use plerkle_serialization::{
    root_as_account_info, root_as_compiled_instruction,
    serializer::seralize_encoded_transaction_with_status, AccountInfo, AccountInfoArgs,
    CompiledInstruction as FBCompiledInstruction, CompiledInstructionBuilder,
    InnerInstructionsBuilder, Pubkey as FBPubkey, TransactionInfo, TransactionInfoBuilder,
};
use rand::Rng;
use solana_geyser_plugin_interface::geyser_plugin_interface::ReplicaAccountInfo;
use solana_sdk::{instruction::CompiledInstruction, pubkey::Pubkey};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, InnerInstruction, InnerInstructions,
};
use spl_account_compression::events::{
    AccountCompressionEvent, ApplicationDataEvent, ApplicationDataEventV1,
};
use std::{fs::File, io::BufReader};

pub fn random_program() -> Pubkey {
    Pubkey::new_unique()
}

pub fn random_pubkey() -> Pubkey {
    random_program()
}

pub fn random_data(max: usize) -> Vec<u8> {
    let mut s = rand::thread_rng();
    let x = s.gen_range(1..max);
    let mut data: Vec<u8> = Vec::with_capacity(x);
    for i in 0..x {
        let d: u8 = s.gen_range(0..255);
        data.insert(i, d);
    }
    data
}

pub fn random_u8() -> u8 {
    let mut s = rand::thread_rng();
    s.gen()
}

pub fn random_u8_bound(min: u8, max: u8) -> u8 {
    let mut s = rand::thread_rng();
    s.gen_range(min..max)
}

pub fn random_list(size: usize, elem_max: u8) -> Vec<u8> {
    let mut s = rand::thread_rng();
    let mut data: Vec<u8> = Vec::with_capacity(size);
    for i in 0..size {
        let d: u8 = s.gen_range(0..elem_max);
        data.insert(i, d);
    }
    data
}

pub fn random_list_of<FN, T>(size: usize, fun: FN) -> Vec<T>
where
    FN: Fn(u8) -> T,
{
    let mut s = rand::thread_rng();
    let mut data: Vec<T> = Vec::with_capacity(size);
    for i in 0..size {
        data.insert(i, fun(s.gen()));
    }
    data
}

pub fn build_random_instruction<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    accounts_number_in_transaction: usize,
    number_of_accounts: usize,
) -> WIPOffset<FBCompiledInstruction<'a>> {
    let accounts = random_list(5, random_u8_bound(1, number_of_accounts as u8));
    let accounts = fbb.create_vector(&accounts);
    let data = random_data(10);
    let data = fbb.create_vector(&data);
    let mut s = rand::thread_rng();
    let mut builder = CompiledInstructionBuilder::new(fbb);
    builder.add_data(data);
    builder.add_program_id_index(s.gen_range(0..accounts_number_in_transaction) as u8);
    builder.add_accounts(accounts);
    builder.finish()
}

pub fn build_random_transaction(mut fbb: FlatBufferBuilder) -> FlatBufferBuilder {
    let mut s = rand::thread_rng();
    let mut outer_instructions = vec![];
    let mut inner_instructions = vec![];
    for _ in 0..s.gen_range(2..7) {
        outer_instructions.push(build_random_instruction(&mut fbb, 10, 3));

        let mut indexed_inner_instructions = vec![];
        for _ in 0..s.gen_range(2..7) {
            let ix = build_random_instruction(&mut fbb, 10, 3);
            indexed_inner_instructions.push(ix);
        }

        let indexed_inner_instructions = fbb.create_vector(&indexed_inner_instructions);
        let mut builder = InnerInstructionsBuilder::new(&mut fbb);
        builder.add_index(s.gen_range(0..7));
        builder.add_instructions(indexed_inner_instructions);
        inner_instructions.push(builder.finish());
    }

    let outer_instructions = fbb.create_vector(&outer_instructions);
    let inner_instructions = fbb.create_vector(&inner_instructions);
    let account_keys = random_list_of(10, |_| FBPubkey(random_pubkey().to_bytes()));
    let account_keys = fbb.create_vector(&account_keys);
    let mut builder = TransactionInfoBuilder::new(&mut fbb);
    let slot = s.gen();
    builder.add_outer_instructions(outer_instructions);
    builder.add_is_vote(false);
    builder.add_inner_instructions(inner_instructions);
    builder.add_account_keys(account_keys);
    builder.add_slot(slot);
    builder.add_seen_at(s.gen());
    let builder = builder.finish();
    fbb.finish_minimal(builder);
    fbb
}

pub fn get_programs(txn_info: TransactionInfo) -> Vec<Pubkey> {
    let mut outer_keys: Vec<Pubkey> = txn_info
        .outer_instructions()
        .unwrap()
        .iter()
        .map(|ix| {
            Pubkey::new_from_array(
                txn_info
                    .account_keys()
                    .unwrap()
                    .iter()
                    .collect::<Vec<_>>()
                    .get(ix.program_id_index() as usize)
                    .unwrap()
                    .0,
            )
        })
        .collect();
    let mut inner = vec![];
    let inner_keys = txn_info
        .inner_instructions()
        .unwrap()
        .iter()
        .fold(&mut inner, |ix, curr| {
            for p in curr.instructions().unwrap() {
                ix.push(Pubkey::new_from_array(
                    txn_info
                        .account_keys()
                        .unwrap()
                        .iter()
                        .collect::<Vec<_>>()
                        .get(p.program_id_index() as usize)
                        .unwrap()
                        .0,
                ))
            }
            ix
        });
    outer_keys.append(inner_keys);
    outer_keys.dedup();
    outer_keys
}

pub fn build_instruction<'a>(
    fbb: &'a mut FlatBufferBuilder<'a>,
    data: &[u8],
    account_indexes: &[u8],
) -> Result<CompiledInstruction, flatbuffers::InvalidFlatbuffer> {
    let accounts_vec = fbb.create_vector(account_indexes);
    let ix_data = fbb.create_vector(data);
    let mut builder = CompiledInstructionBuilder::new(fbb);
    builder.add_accounts(accounts_vec);
    builder.add_program_id_index(0);
    builder.add_data(ix_data);
    let offset = builder.finish();
    fbb.finish_minimal(offset);
    let data = fbb.finished_data();

    let cix = root_as_compiled_instruction(data)?;
    Ok(CompiledInstruction {
        program_id_index: cix.program_id_index(),
        accounts: cix
            .accounts()
            .expect("failed to deserialize accounts")
            .bytes()
            .to_vec(),
        data: cix
            .data()
            .expect("failed to deserialize data")
            .bytes()
            .to_vec(),
    })
}

pub fn build_account_update<'a>(
    fbb: &'a mut FlatBufferBuilder<'a>,
    account: &ReplicaAccountInfo,
    slot: u64,
    is_startup: bool,
) -> Result<AccountInfo<'a>, flatbuffers::InvalidFlatbuffer> {
    // Serialize vector data.
    let pubkey = FBPubkey::from(account.pubkey);
    let owner = FBPubkey::from(account.owner);

    // Don't serialize a zero-length data slice.
    let data = if !account.data.is_empty() {
        Some(fbb.create_vector(account.data))
    } else {
        None
    };

    // Serialize everything into Account Info table.
    let account_info = AccountInfo::create(
        fbb,
        &AccountInfoArgs {
            pubkey: Some(&pubkey),
            lamports: account.lamports,
            owner: Some(&owner),
            executable: account.executable,
            rent_epoch: account.rent_epoch,
            data,
            write_version: account.write_version,
            slot,
            is_startup,
            seen_at: 0,
        },
    );

    // Finalize buffer
    fbb.finish(account_info, None);
    let data = fbb.finished_data();
    root_as_account_info(data)
}

pub fn build_random_account_update<'a>(
    fbb: &'a mut FlatBufferBuilder<'a>,
    data: &[u8],
) -> Result<AccountInfo<'a>, flatbuffers::InvalidFlatbuffer> {
    // Create a `ReplicaAccountInfo` to store the account update.
    // All fields except caller-specified `data` are just random values.
    let replica_account_info = ReplicaAccountInfo {
        pubkey: &random_pubkey().to_bytes()[..],
        lamports: 1,
        owner: &random_pubkey().to_bytes()[..],
        executable: false,
        rent_epoch: 1000,
        data,
        write_version: 1,
    };

    // Flatbuffer serialize the `ReplicaAccountInfo`.
    build_account_update(fbb, &replica_account_info, 0, false)
}

pub fn build_txn_from_fixture(
    fixture_name: String,
    fbb: FlatBufferBuilder,
) -> Result<FlatBufferBuilder, BlockbusterError> {
    let file = File::open(format!(
        "{}/tests/fixtures/{}.json",
        env!("CARGO_MANIFEST_DIR"),
        fixture_name
    ))
    .unwrap();
    let reader = BufReader::new(file);
    let ectxn: EncodedConfirmedTransactionWithStatusMeta = serde_json::from_reader(reader).unwrap();
    Ok(seralize_encoded_transaction_with_status(fbb, ectxn)
        .expect("failed serialize encoded tx with status"))
}

#[allow(clippy::too_many_arguments)]
pub fn build_bubblegum_bundle<'a>(
    fbb1: &'a mut FlatBufferBuilder<'a>,
    fbb2: &'a mut FlatBufferBuilder<'a>,
    fbb3: &'a mut FlatBufferBuilder<'a>,
    fbb4: &'a mut FlatBufferBuilder<'a>,
    accounts: &'a [Pubkey],
    account_indexes: &'a [u8],
    ix_data: &'a [u8],
    lse: LeafSchemaEvent,
    cs_event: AccountCompressionEvent,
) -> InstructionBundle<'a> {
    let lse_versioned = ApplicationDataEventV1 {
        application_data: lse.try_to_vec().unwrap(),
    };
    let lse_event =
        AccountCompressionEvent::ApplicationData(ApplicationDataEvent::V1(lse_versioned));
    let outer_ix = build_instruction(fbb1, ix_data, account_indexes).unwrap();

    let lse = lse_event.try_to_vec().unwrap();
    let noop_bgum = spl_noop::instruction(lse).data;
    let ix = build_instruction(fbb2, &noop_bgum, account_indexes).unwrap();
    let noop_bgum_ix: IxPair = (spl_noop::id(), Box::leak(Box::new(ix)));

    // The Compression Instruction here doesnt matter only the noop but we add it here to ensure we are validating that one Account compression event is happening after Bubblegum
    let ix = build_instruction(fbb3, &[0; 0], account_indexes).unwrap();
    let gummy_roll_ix: IxPair = (spl_account_compression::id(), Box::leak(Box::new(ix)));

    let cs = cs_event.try_to_vec().unwrap();
    let noop_compression = spl_noop::instruction(cs).data;
    let ix = build_instruction(fbb4, &noop_compression, account_indexes).unwrap();
    let noop_compression_ix: IxPair = (spl_noop::id(), Box::leak(Box::new(ix)));

    let inner_ix = vec![noop_bgum_ix, gummy_roll_ix, noop_compression_ix];

    // `Box::leak` is ok for tests
    InstructionBundle {
        program: mpl_bubblegum::ID,
        inner_ix: Some(Box::leak(Box::new(inner_ix))),
        keys: accounts,
        instruction: Some(Box::leak(Box::new(outer_ix))),
        ..Default::default()
    }
}

pub fn parse_fb(
    tx_info: &TransactionInfo,
) -> (
    Vec<Pubkey>,
    Vec<CompiledInstruction>,
    Vec<InnerInstructions>,
) {
    let mut account_keys = vec![];
    for key in tx_info.account_keys().iter().flatten() {
        account_keys.push(Pubkey::try_from(key.0.as_slice()).expect("valid key from FlatBuffer"));
    }

    let mut message_instructions = vec![];
    for cix in tx_info
        .outer_instructions()
        .expect("valid outer_instructions")
    {
        message_instructions.push(CompiledInstruction {
            program_id_index: cix.program_id_index(),
            accounts: cix.accounts().expect("valid accounts").bytes().to_vec(),
            data: cix.data().expect("valid data").bytes().to_vec(),
        });
    }

    let mut meta_inner_instructions = vec![];
    if let Some(ixs) = tx_info.compiled_inner_instructions() {
        for ix in ixs {
            let mut instructions = vec![];
            for ix in ix.instructions().expect("valid instructions") {
                let cix = ix.compiled_instruction().expect("valid instruction");
                instructions.push(InnerInstruction {
                    instruction: CompiledInstruction {
                        program_id_index: cix.program_id_index(),
                        accounts: cix.accounts().expect("valid accounts").bytes().to_vec(),
                        data: cix.data().expect("valid data").bytes().to_vec(),
                    },
                    stack_height: Some(ix.stack_height() as u32),
                })
            }

            meta_inner_instructions.push(InnerInstructions {
                index: ix.index(),
                instructions,
            })
        }
    } else if let Some(ixs) = tx_info.inner_instructions() {
        for ix in ixs {
            let mut instructions = vec![];
            for cix in ix.instructions().expect("valid instructions") {
                instructions.push(InnerInstruction {
                    instruction: CompiledInstruction {
                        program_id_index: cix.program_id_index(),
                        accounts: cix.accounts().expect("valid accounts").bytes().to_vec(),
                        data: cix.data().expect("valid data").bytes().to_vec(),
                    },
                    stack_height: Some(0),
                })
            }

            meta_inner_instructions.push(InnerInstructions {
                index: ix.index(),
                instructions,
            })
        }
    } else {
        panic!("expect valid compiled_inner_instructions/inner_instructions")
    }

    (account_keys, message_instructions, meta_inner_instructions)
}
