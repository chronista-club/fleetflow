//! Cloud provider error types

use thiserror::Error;

/// Cloud provider errors
#[derive(Error, Debug)]
pub enum CloudError {
    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Resource already exists: {0}")]
    ResourceAlreadyExists(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("State file error: {0}")]
    StateError(String),

    #[error("Lock acquisition failed: {0}")]
    LockError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CloudError>;
