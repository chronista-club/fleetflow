//! Sakura Cloud provider error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SakuraError {
    #[error("usacloud not found. Please install: brew install usacloud")]
    UsacloudNotFound,

    #[error("usacloud authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("usacloud command failed: {0}")]
    CommandFailed(String),

    #[error("Server not found: {0}")]
    ServerNotFound(String),

    #[error("Disk not found: {0}")]
    DiskNotFound(String),

    #[error("Invalid zone: {0}")]
    InvalidZone(String),

    #[error("Invalid plan: {0}")]
    InvalidPlan(String),

    #[error("SSH key not found: {0}")]
    SshKeyNotFound(String),

    #[error("Resource creation failed: {0}")]
    CreationFailed(String),

    #[error("Resource deletion failed: {0}")]
    DeletionFailed(String),

    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Cloud error: {0}")]
    CloudError(#[from] fleetflow_cloud::CloudError),
}

pub type Result<T> = std::result::Result<T, SakuraError>;
