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
    #[error("Unexpected tree depth={0} and max size={1}")]
    UnexpectedTreeSize(u32, u32),
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

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum RollupValidationError {
    #[error("PDACheckFail: expected: {0}, got: {1}")]
    PDACheckFail(String, String),
    #[error("InvalidDataHash: expected: {0}, got: {1}")]
    InvalidDataHash(String, String),
    #[error("InvalidCreatorsHash: expected: {0}, got: {1}")]
    InvalidCreatorsHash(String, String),
    #[error("InvalidRoot: expected: {0}, got: {1}")]
    InvalidRoot(String, String),
    #[error("CannotCreateMerkleTree: depth [{0}], size [{1}]")]
    CannotCreateMerkleTree(u32, u32),
    #[error("NoRelevantRolledMint: index {0}")]
    NoRelevantRolledMint(u64),
    #[error("WrongAssetPath: id {0}")]
    WrongAssetPath(String),
    #[error("StdIo {0}")]
    StdIo(String),
    #[error("WrongTreeIdForChangeLog: asset: {0}, expected: {1}, got: {2}")]
    WrongTreeIdForChangeLog(String, String, String),
    #[error("WrongChangeLogIndex: asset: {0}, expected: {0}, got: {1}")]
    WrongChangeLogIndex(String, u32, u32),
    #[error("SplCompression: {0}")]
    SplCompression(#[from] spl_account_compression::ConcurrentMerkleTreeError),
    #[error("Anchor {0}")]
    Anchor(#[from] anchor_lang::error::Error),
    #[error("FileChecksumMismatch: expected {0}, actual file hash {1}")]
    FileChecksumMismatch(String, String),
}

impl From<std::io::Error> for RollupValidationError {
    fn from(err: std::io::Error) -> Self {
        RollupValidationError::StdIo(err.to_string())
    }
}
