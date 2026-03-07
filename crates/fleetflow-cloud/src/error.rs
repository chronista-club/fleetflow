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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_provider_not_found() {
        let err = CloudError::ProviderNotFound("sakura".to_string());
        assert_eq!(err.to_string(), "Provider not found: sakura");
    }

    #[test]
    fn test_error_display_resource_not_found() {
        let err = CloudError::ResourceNotFound("srv-01".to_string());
        assert_eq!(err.to_string(), "Resource not found: srv-01");
    }

    #[test]
    fn test_error_display_resource_already_exists() {
        let err = CloudError::ResourceAlreadyExists("srv-01".to_string());
        assert_eq!(err.to_string(), "Resource already exists: srv-01");
    }

    #[test]
    fn test_error_display_authentication_failed() {
        let err = CloudError::AuthenticationFailed("invalid token".to_string());
        assert_eq!(err.to_string(), "Authentication failed: invalid token");
    }

    #[test]
    fn test_error_display_api_error() {
        let err = CloudError::ApiError("500 internal".to_string());
        assert_eq!(err.to_string(), "API error: 500 internal");
    }

    #[test]
    fn test_error_display_command_failed() {
        let err = CloudError::CommandFailed("exit code 1".to_string());
        assert_eq!(err.to_string(), "Command execution failed: exit code 1");
    }

    #[test]
    fn test_error_display_invalid_config() {
        let err = CloudError::InvalidConfig("missing field".to_string());
        assert_eq!(err.to_string(), "Invalid configuration: missing field");
    }

    #[test]
    fn test_error_display_state_error() {
        let err = CloudError::StateError("corrupt file".to_string());
        assert_eq!(err.to_string(), "State file error: corrupt file");
    }

    #[test]
    fn test_error_display_lock_error() {
        let err = CloudError::LockError("already locked".to_string());
        assert_eq!(err.to_string(), "Lock acquisition failed: already locked");
    }

    #[test]
    fn test_error_display_timeout() {
        let err = CloudError::Timeout("30s elapsed".to_string());
        assert_eq!(err.to_string(), "Timeout: 30s elapsed");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let cloud_err: CloudError = io_err.into();
        assert!(cloud_err.to_string().contains("file missing"));
    }

    #[test]
    fn test_error_from_json() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let cloud_err: CloudError = json_err.into();
        assert!(cloud_err.to_string().starts_with("JSON error:"));
    }
}
