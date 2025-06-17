use log::{debug, error};

use {jsonrpsee::core::Error as RpcError, jsonrpsee::types::error::CallError, thiserror::Error};

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
pub enum DasApiError {
    #[error("Config Missing or Error: {0}")]
    ConfigurationError(String),
    #[error("Server Failed to Start")]
    ServerStartError(#[from] RpcError),
    #[error("Database Connection Failed")]
    DatabaseConnectionError(#[from] sqlx::Error),
    #[error("Pubkey Validation Err: {0} is invalid")]
    PubkeyValidationError(String),
    #[error("Validation Error: {0}")]
    ValidationError(String),
    #[error("Database Error: {0}")]
    DatabaseError(#[from] sea_orm::DbErr),
    #[error("Pagination Error. Only one pagination parameter supported per query.")]
    PaginationError,
    #[error("Pagination Error. No Pagination Method Selected")]
    PaginationEmptyError,
    #[error("Deserialization error: {0}")]
    DeserializationError(#[from] serde_json::Error),
    #[error("Batch Size Error. Batch size should not be greater than 1000.")]
    BatchSizeExceededError,
    #[error("Pagination Error. Limit should not be greater than 1000.")]
    PaginationExceededError,
    #[error("Cursor Validation Err: {0} is invalid")]
    CursorValidationError(String),
    #[error("Pagination Sorting Error. Only sorting based on id is supported for this pagination option.")]
    PaginationSortingValidationError,
    #[error("Invalid Program Id: {0}")]
    InvalidProgramId(String),
}

impl DasApiError {
    pub const fn to_error_code(&self) -> &'static str {
        match self {
            DasApiError::ConfigurationError(_) => "CONFIGURATION_ERROR",
            DasApiError::ServerStartError(_) => "SERVER_START_ERROR",
            DasApiError::DatabaseConnectionError(_) => "DB_CONNECTION_ERROR",
            DasApiError::PubkeyValidationError(_) => "PUBKEY_VALIDATION_ERROR",
            DasApiError::ValidationError(_) => "VALIDATION_ERROR",
            DasApiError::DatabaseError(_) => "DATABASE_ERROR",
            DasApiError::PaginationError => "PAGINATION_ERROR",
            DasApiError::PaginationEmptyError => "PAGINATION_EMPTY_ERROR",
            DasApiError::DeserializationError(_) => "DESERIALIZATION_ERROR",
            DasApiError::BatchSizeExceededError => "BATCH_SIZE_EXCEEDED_ERROR",
            DasApiError::PaginationExceededError => "PAGINATION_EXCEEDED_ERROR",
            DasApiError::CursorValidationError(_) => "CURSOR_VALIDATION_ERROR",
            DasApiError::PaginationSortingValidationError => "PAGINATION_SORTING_VALIDATION_ERROR",
            DasApiError::InvalidProgramId(_) => "INVALID_PROGRAM_ID",
        }
    }
}

impl From<DasApiError> for RpcError {
    fn from(error: DasApiError) -> RpcError {
        match error {
            DasApiError::ValidationError(_) => {
                debug!("{}", error);
            }
            _ => {
                error!("{}", error);
            }
        }
        RpcError::Call(CallError::from_std_error(error))
    }
}
