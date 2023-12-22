use crate::tasks::TaskData;
use blockbuster::error::BlockbusterError;
use plerkle_messenger::MessengerError;
use plerkle_serialization::error::PlerkleSerializationError;
use sea_orm::{DbErr, TransactionError};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum IngesterError {
    #[error("ChangeLog Event Malformed")]
    ChangeLogEventMalformed,
    #[error("Compressed Asset Event Malformed")]
    CompressedAssetEventMalformed,
    #[error("Network Error: {0}")]
    BatchInitNetworkingError(String),
    #[error("Error writing batch files")]
    BatchInitIOError,
    #[error("Storage listener error: ({msg})")]
    StorageListenerError { msg: String },
    #[error("Storage Write Error: {0}")]
    StorageWriteError(String),
    #[error("NotImplemented")]
    NotImplemented,
    #[error("Deserialization Error: {0}")]
    DeserializationError(String),
    #[error("Task Manager Error: {0}")]
    TaskManagerError(String),
    #[error("Missing or invalid configuration: ({msg})")]
    ConfigurationError { msg: String },
    #[error("Error getting RPC data: {0}")]
    RpcGetDataError(String),
    #[error("RPC returned data in unsupported format: {0}")]
    RpcDataUnsupportedFormat(String),
    #[error("Data serializaton error: {0}")]
    SerializatonError(String),
    #[error("Messenger error; {0}")]
    MessengerError(String),
    #[error("Blockbuster Parsing error: {0}")]
    ParsingError(String),
    #[error("Database Error: {0}")]
    DatabaseError(String),
    #[error("Unknown Task Type: {0}")]
    UnknownTaskType(String),
    #[error("BG Task Manager Not Started")]
    TaskManagerNotStarted,
    #[error("Unrecoverable task error: {0}")]
    UnrecoverableTaskError(String),
    #[error("Cache Storage Write Error: {0}")]
    CacheStorageWriteError(String),
    #[error("HttpError {status_code}")]
    HttpError { status_code: String },
    #[error("AssetIndex Error {0}")]
    AssetIndexError(String),
    #[error("TryFromInt Error {0}")]
    TryFromInt(#[from] std::num::TryFromIntError),
    #[error("Chrono FixedOffset Error")]
    ChronoFixedOffset,
    #[error("Pubkey parse")]
    ParsePubkey,
    #[error("Signature parse")]
    ParseSignature(#[from] solana_sdk::signature::ParseSignatureError),
    #[error("Missing Path at index for change log event")]
    MissingChangeLogPath,
}

impl From<reqwest::Error> for IngesterError {
    fn from(err: reqwest::Error) -> Self {
        IngesterError::BatchInitNetworkingError(err.to_string())
    }
}

impl From<stretto::CacheError> for IngesterError {
    fn from(err: stretto::CacheError) -> Self {
        IngesterError::CacheStorageWriteError(err.to_string())
    }
}

impl From<serde_json::Error> for IngesterError {
    fn from(_err: serde_json::Error) -> Self {
        IngesterError::SerializatonError("JSON ERROR".to_string())
    }
}

impl From<BlockbusterError> for IngesterError {
    fn from(err: BlockbusterError) -> Self {
        IngesterError::ParsingError(err.to_string())
    }
}

impl From<std::io::Error> for IngesterError {
    fn from(_err: std::io::Error) -> Self {
        IngesterError::BatchInitIOError
    }
}

impl From<DbErr> for IngesterError {
    fn from(e: DbErr) -> Self {
        IngesterError::StorageWriteError(e.to_string())
    }
}

impl From<TransactionError<IngesterError>> for IngesterError {
    fn from(e: TransactionError<IngesterError>) -> Self {
        IngesterError::StorageWriteError(e.to_string())
    }
}

impl From<SendError<TaskData>> for IngesterError {
    fn from(err: SendError<TaskData>) -> Self {
        IngesterError::TaskManagerError(format!("Could not create task: {:?}", err.to_string()))
    }
}

impl From<MessengerError> for IngesterError {
    fn from(e: MessengerError) -> Self {
        IngesterError::MessengerError(e.to_string())
    }
}

impl From<PlerkleSerializationError> for IngesterError {
    fn from(e: PlerkleSerializationError) -> Self {
        IngesterError::SerializatonError(e.to_string())
    }
}
