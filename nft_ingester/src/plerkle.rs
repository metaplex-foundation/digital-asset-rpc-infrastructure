use {
    flatbuffers::{ForwardsUOffset, Vector},
    plerkle_serialization::{
        CompiledInnerInstructions as FBCompiledInnerInstructions,
        CompiledInstruction as FBCompiledInstruction, Pubkey as FBPubkey,
    },
    program_transformers::error::{ProgramTransformerError, ProgramTransformerResult},
    solana_sdk::{instruction::CompiledInstruction, pubkey::Pubkey, signature::Signature},
    solana_transaction_status::{InnerInstruction, InnerInstructions},
};

fn deser_err() -> ProgramTransformerError {
    ProgramTransformerError::DeserializationError("Could not deserialize data".to_owned())
}

pub fn parse_pubkey(pubkey: Option<&FBPubkey>) -> ProgramTransformerResult<Pubkey> {
    Ok(Pubkey::try_from(pubkey.ok_or_else(deser_err)?.0.as_slice())
        .expect("valid key from FlatBuffer"))
}

pub fn parse_slice(data: Option<Vector<'_, u8>>) -> ProgramTransformerResult<&[u8]> {
    data.map(|data| data.bytes()).ok_or_else(deser_err)
}

pub fn parse_signature(data: Option<&str>) -> ProgramTransformerResult<Signature> {
    data.ok_or_else(deser_err)?
        .parse()
        .map_err(|_error| deser_err())
}

pub fn parse_account_keys(
    keys: Option<Vector<'_, FBPubkey>>,
) -> ProgramTransformerResult<Vec<Pubkey>> {
    let mut account_keys = vec![];
    for key in keys.ok_or_else(deser_err)? {
        account_keys.push(Pubkey::try_from(key.0.as_slice()).expect("valid key from FlatBuffer"));
    }
    Ok(account_keys)
}

pub fn parse_message_instructions(
    vec_cix: Option<Vector<'_, ForwardsUOffset<FBCompiledInstruction>>>,
) -> ProgramTransformerResult<Vec<CompiledInstruction>> {
    let mut message_instructions = vec![];
    for cix in vec_cix.ok_or_else(deser_err)? {
        message_instructions.push(CompiledInstruction {
            program_id_index: cix.program_id_index(),
            accounts: cix.accounts().ok_or_else(deser_err)?.bytes().to_vec(),
            data: cix.data().ok_or_else(deser_err)?.bytes().to_vec(),
        })
    }
    Ok(message_instructions)
}

pub fn parse_meta_inner_instructions(
    vec_ixs: Option<Vector<'_, ForwardsUOffset<FBCompiledInnerInstructions>>>,
) -> ProgramTransformerResult<Vec<InnerInstructions>> {
    let mut meta_inner_instructions = vec![];
    for ixs in vec_ixs.ok_or_else(deser_err)? {
        let mut instructions = vec![];
        for ix in ixs.instructions().ok_or_else(deser_err)? {
            let cix = ix.compiled_instruction().ok_or_else(deser_err)?;
            instructions.push(InnerInstruction {
                instruction: CompiledInstruction {
                    program_id_index: cix.program_id_index(),
                    accounts: cix.accounts().ok_or_else(deser_err)?.bytes().to_vec(),
                    data: cix.data().ok_or_else(deser_err)?.bytes().to_vec(),
                },
                stack_height: Some(ix.stack_height() as u32),
            });
        }
        meta_inner_instructions.push(InnerInstructions {
            index: ixs.index(),
            instructions,
        })
    }
    Ok(meta_inner_instructions)
}
