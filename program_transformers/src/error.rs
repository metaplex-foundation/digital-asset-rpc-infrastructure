use {blockbuster::error::BlockbusterError, sea_orm::DbErr};

pub type ProgramTransformerResult<T> = Result<T, ProgramTransformerError>;

#[derive(Debug, thiserror::Error)]
pub enum ProgramTransformerError {
    #[error("ChangeLog Event Malformed")]
    ChangeLogEventMalformed,
    #[error("Storage Write Error: {0}")]
    StorageWriteError(String),
    #[error("NotImplemented")]
    NotImplemented,
    #[error("Deserialization Error: {0}")]
    DeserializationError(String),
    #[error("Data serializaton error: {0}")]
    SerializatonError(String),
    #[error("Blockbuster Parsing error: {0}")]
    ParsingError(String),
    #[error("Database Error: {0}")]
    DatabaseError(String),
    #[error("AssetIndex Error {0}")]
    AssetIndexError(String),
    #[error("Failed to notify about download metadata: {0}")]
    DownloadMetadataNotify(Box<dyn std::error::Error + Send + Sync>),
}

impl From<BlockbusterError> for ProgramTransformerError {
    fn from(err: BlockbusterError) -> Self {
        ProgramTransformerError::ParsingError(err.to_string())
    }
}

impl From<DbErr> for ProgramTransformerError {
    fn from(e: DbErr) -> Self {
        ProgramTransformerError::StorageWriteError(e.to_string())
    }
}
