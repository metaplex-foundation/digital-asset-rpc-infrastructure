use {
    plerkle_serialization::deserializer::SolanaDeserializerError,
    program_transformers::error::ProgramTransformerError,
};

pub fn into_program_transformer_err(e: SolanaDeserializerError) -> ProgramTransformerError {
    ProgramTransformerError::DeserializationError(e.to_string())
}
