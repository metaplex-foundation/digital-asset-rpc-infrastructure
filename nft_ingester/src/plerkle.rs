use {
    flatbuffers::{ForwardsUOffset, Vector},
    plerkle_serialization::{
        CompiledInnerInstructions as FBCompiledInnerInstructions,
        CompiledInstruction as FBCompiledInstruction, InnerInstructions as FBInnerInstructions,
        Pubkey as FBPubkey,
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
    keys.ok_or_else(deser_err).map(|keys| {
        keys.iter()
            .map(|key| Pubkey::try_from(key.0.as_slice()).expect("valid key from FlatBuffer"))
            .collect()
    })
}

pub fn parse_message_instructions(
    vec_cix: Option<Vector<'_, ForwardsUOffset<FBCompiledInstruction>>>,
) -> ProgramTransformerResult<Vec<CompiledInstruction>> {
    vec_cix.ok_or_else(deser_err).and_then(|vec| {
        vec.iter()
            .map(|cix| {
                Ok(CompiledInstruction {
                    program_id_index: cix.program_id_index(),
                    accounts: cix.accounts().ok_or_else(deser_err)?.bytes().to_vec(),
                    data: cix.data().ok_or_else(deser_err)?.bytes().to_vec(),
                })
            })
            .collect()
    })
}

pub fn parse_meta_inner_instructions(
    vec_ciixs: Option<Vector<'_, ForwardsUOffset<FBCompiledInnerInstructions>>>,
    vec_iixs: Option<Vector<'_, ForwardsUOffset<FBInnerInstructions>>>,
) -> ProgramTransformerResult<Vec<InnerInstructions>> {
    if let Some(vec_ciixs) = vec_ciixs {
        vec_ciixs
            .iter()
            .map(|ciix| {
                Ok(InnerInstructions {
                    index: ciix.index(),
                    instructions: ciix
                        .instructions()
                        .ok_or_else(deser_err)?
                        .iter()
                        .map(|ix| {
                            let cix = ix.compiled_instruction().ok_or_else(deser_err)?;
                            Ok(InnerInstruction {
                                instruction: CompiledInstruction {
                                    program_id_index: cix.program_id_index(),
                                    accounts: cix
                                        .accounts()
                                        .ok_or_else(deser_err)?
                                        .bytes()
                                        .to_vec(),
                                    data: cix.data().ok_or_else(deser_err)?.bytes().to_vec(),
                                },
                                stack_height: Some(ix.stack_height() as u32),
                            })
                        })
                        .collect::<Result<_, ProgramTransformerError>>()?,
                })
            })
            .collect()
    } else if let Some(vec_iixs) = vec_iixs {
        vec_iixs
            .iter()
            .map(|iixs| {
                Ok(InnerInstructions {
                    index: iixs.index(),
                    instructions: iixs
                        .instructions()
                        .expect("valid instructions")
                        .iter()
                        .map(|cix| InnerInstruction {
                            instruction: CompiledInstruction {
                                program_id_index: cix.program_id_index(),
                                accounts: cix.accounts().expect("valid accounts").bytes().to_vec(),
                                data: cix.data().expect("valid data").bytes().to_vec(),
                            },
                            stack_height: Some(0),
                        })
                        .collect(),
                })
            })
            .collect()
    } else {
        Err(deser_err())
    }
}
