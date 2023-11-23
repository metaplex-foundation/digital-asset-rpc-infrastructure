use {
    crate::tasks::TaskData, plerkle_messenger::MessengerError,
    plerkle_serialization::error::PlerkleSerializationError, sea_orm::DbErr,
    tokio::sync::mpsc::error::SendError,
};

#[derive(Debug, thiserror::Error)]
pub enum IngesterError {
    #[error("Network Error: {0}")]
    BatchInitNetworkingError(String),
    #[error("Storage Write Error: {0}")]
    StorageWriteError(String),
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
    #[error("BG Task Manager Not Started")]
    TaskManagerNotStarted,
    #[error("Unrecoverable task error: {0}")]
    UnrecoverableTaskError(String),
    #[error("Cache Storage Write Error: {0}")]
    CacheStorageWriteError(String),
    #[error("HttpError {status_code}")]
    HttpError { status_code: String },
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

impl From<DbErr> for IngesterError {
    fn from(e: DbErr) -> Self {
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
