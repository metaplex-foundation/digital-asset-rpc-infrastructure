use {jsonrpsee::core::Error as RpcError, jsonrpsee::types::error::CallError, thiserror::Error};

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
pub enum DasApiError {
    #[error("Config Missing or Error {0}")]
    ConfigurationError(String),
    #[error("Server Failed to Start")]
    ServerStartError(#[from] RpcError),
    #[error("Database Connection Failed")]
    DatabaseConnectionError(#[from] sqlx::Error),
    #[error("Pubkey Validation Err {0} is invalid")]
    PubkeyValidationError(String),
    #[error("Validation Error {0}")]
    ValidationError(String),
    #[error("Database Error {0}")]
    DatabaseError(#[from] sea_orm::DbErr),
    #[error("Pagination Error. Only one pagination parameter supported per query.")]
    PaginationError,
    #[error("Pagination Error. No Pagination Method Selected")]
    PaginationEmptyError,
    #[error("Deserialization error: {0}")]
    DeserializationError(#[from] serde_json::Error),
}


impl From<DasApiError> for RpcError {
    fn from(value: DasApiError) -> Self {
        println!("{}", value);
        RpcError::Call(CallError::from_std_error(value))
    }
}
