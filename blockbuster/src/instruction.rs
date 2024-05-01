use solana_sdk::{instruction::CompiledInstruction, pubkey::Pubkey};
use solana_transaction_status::InnerInstructions;
use std::collections::{HashSet, VecDeque};

pub type IxPair<'a> = (Pubkey, &'a CompiledInstruction);

#[derive(Debug, Clone, Copy)]
pub struct InstructionBundle<'a> {
    pub txn_id: &'a str,
    pub program: Pubkey,
    pub instruction: Option<&'a CompiledInstruction>,
    pub inner_ix: Option<&'a [IxPair<'a>]>,
    pub keys: &'a [Pubkey],
    pub slot: u64,
}

impl<'a> Default for InstructionBundle<'a> {
    fn default() -> Self {
        InstructionBundle {
            txn_id: "",
            program: Pubkey::new_from_array([0; 32]),
            instruction: None,
            inner_ix: None,
            keys: &[],
            slot: 0,
        }
    }
}

pub fn order_instructions<'a>(
    programs: &HashSet<Pubkey>,
    account_keys: &[Pubkey],
    message_instructions: &'a [CompiledInstruction],
    meta_inner_instructions: &'a [InnerInstructions],
) -> VecDeque<(IxPair<'a>, Option<Vec<IxPair<'a>>>)> {
    let mut ordered_ixs: VecDeque<(IxPair, Option<Vec<IxPair>>)> = VecDeque::new();

    // Get inner instructions.
    for (outer_instruction_index, message_instruction) in message_instructions.iter().enumerate() {
        let non_hoisted_inner_instruction = meta_inner_instructions
            .iter()
            .filter_map(|ix| {
                (ix.index == outer_instruction_index as u8).then_some(&ix.instructions)
            })
            .flatten()
            .map(|inner_ix| {
                let cix = &inner_ix.instruction;
                (account_keys[cix.program_id_index as usize], cix)
            })
            .collect::<Vec<IxPair>>();

        let hoisted = hoist_known_programs(programs, &non_hoisted_inner_instruction);
        ordered_ixs.extend(hoisted);

        if let Some(outer_program_id) =
            account_keys.get(message_instruction.program_id_index as usize)
        {
            if programs.contains(outer_program_id) {
                ordered_ixs.push_back((
                    (*outer_program_id, message_instruction),
                    Some(non_hoisted_inner_instruction),
                ));
            }
        } else {
            eprintln!("outer program id deserialization error");
        }
    }
    ordered_ixs
}

fn hoist_known_programs<'a>(
    programs: &HashSet<Pubkey>,
    ix_pairs: &[IxPair<'a>],
) -> Vec<(IxPair<'a>, Option<Vec<IxPair<'a>>>)> {
    ix_pairs
        .iter()
        .enumerate()
        .filter(|&(_index, &(pid, _ci))| programs.contains(&pid))
        .map(|(index, &(pid, ci))| {
            let inner_copy = ix_pairs
                .iter()
                .skip(index + 1)
                .take_while(|&&(inner_pid, _)| inner_pid != pid)
                .cloned()
                .collect::<Vec<IxPair<'a>>>();
            ((pid, ci), Some(inner_copy))
        })
        .collect()
}
