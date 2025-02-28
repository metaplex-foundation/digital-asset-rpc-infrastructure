use std::io::Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlockbusterError {
    #[error("Instruction Data Parsing Error")]
    InstructionParsingError,
    #[error("IO Error {0}")]
    IOError(String),
    #[error("Could not deserialize data")]
    DeserializationError,
    #[error("Missing Bubblegum event data")]
    MissingBubblegumEventData,
    #[error("Data length is invalid.")]
    InvalidDataLength,
    #[error("Unknown anchor account discriminator.")]
    UnknownAccountDiscriminator,
    #[error("Account type is not valid")]
    InvalidAccountType,
    #[error("Master edition version is invalid")]
    FailedToDeserializeToMasterEdition,
    #[error("Uninitialized account type")]
    UninitializedAccount,
    #[error("Account Type Not implemented")]
    AccountTypeNotImplemented,
    #[error("Instruction Type Not implemented")]
    InstructionTypeNotImplemented,
    #[error("Could not deserialize data: {0}")]
    CustomDeserializationError(String),
}

impl From<std::io::Error> for BlockbusterError {
    fn from(err: Error) -> Self {
        BlockbusterError::IOError(err.to_string())
    }
}
