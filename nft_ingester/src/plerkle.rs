use {
    plerkle_serialization::deserializer::*,
    program_transformers::{error::ProgramTransformerError, AccountInfo, TransactionInfo},
};

pub fn into_program_transformer_err(e: PlerkleDeserializerError) -> ProgramTransformerError {
    ProgramTransformerError::DeserializationError(e.to_string())
}

#[derive(thiserror::Error, Clone, Debug)]
pub enum PlerkleDeserializerError {
    #[error("Not found")]
    NotFound,
    #[error("Solana error: {0}")]
    Solana(#[from] SolanaDeserializerError),
}

pub struct PlerkleAccountInfo<'a>(pub plerkle_serialization::AccountInfo<'a>);

impl<'a> TryFrom<PlerkleAccountInfo<'a>> for AccountInfo {
    type Error = PlerkleDeserializerError;

    fn try_from(value: PlerkleAccountInfo) -> Result<Self, Self::Error> {
        let account_info = value.0;

        Ok(Self {
            slot: account_info.slot(),
            pubkey: account_info
                .pubkey()
                .ok_or(PlerkleDeserializerError::NotFound)?
                .try_into()?,
            owner: account_info
                .owner()
                .ok_or(PlerkleDeserializerError::NotFound)?
                .try_into()?,
            data: PlerkleOptionalU8Vector(account_info.data()).try_into()?,
        })
    }
}

pub struct PlerkleTransactionInfo<'a>(pub plerkle_serialization::TransactionInfo<'a>);

impl<'a> TryFrom<PlerkleTransactionInfo<'a>> for TransactionInfo {
    type Error = PlerkleDeserializerError;

    fn try_from(value: PlerkleTransactionInfo<'a>) -> Result<Self, Self::Error> {
        let tx_info = value.0;

        let slot = tx_info.slot();
        let signature = PlerkleOptionalStr(tx_info.signature()).try_into()?;
        let account_keys = PlerkleOptionalPubkeyVector(tx_info.account_keys()).try_into()?;
        let message_instructions = PlerkleCompiledInstructionVector(
            tx_info
                .outer_instructions()
                .ok_or(PlerkleDeserializerError::NotFound)?,
        )
        .try_into()?;
        let compiled = tx_info.compiled_inner_instructions();
        let inner = tx_info.inner_instructions();
        let meta_inner_instructions = if let Some(c) = compiled {
            PlerkleCompiledInnerInstructionVector(c).try_into()
        } else {
            PlerkleInnerInstructionsVector(inner.ok_or(PlerkleDeserializerError::NotFound)?)
                .try_into()
        }?;

        Ok(Self {
            slot,
            signature,
            account_keys,
            message_instructions,
            meta_inner_instructions,
        })
    }
}
