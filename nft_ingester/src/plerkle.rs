use {
    flatbuffers::Vector,
    plerkle_serialization::Pubkey as FBPubkey,
    program_transformers::error::{ProgramTransformerError, ProgramTransformerResult},
    solana_sdk::pubkey::Pubkey,
};

pub fn parse_pubkey(pubkey: Option<&FBPubkey>) -> ProgramTransformerResult<Pubkey> {
    Ok(Pubkey::try_from(
        pubkey
            .ok_or_else(|| {
                ProgramTransformerError::DeserializationError(
                    "Could not deserialize data".to_owned(),
                )
            })?
            .0
            .as_slice(),
    )
    .expect("valid key from FlatBuffer"))
}

pub fn parse_vector(data: Option<Vector<'_, u8>>) -> ProgramTransformerResult<&[u8]> {
    data.map(|data| data.bytes()).ok_or_else(|| {
        ProgramTransformerError::DeserializationError("Could not deserialize data".to_owned())
    })
}
