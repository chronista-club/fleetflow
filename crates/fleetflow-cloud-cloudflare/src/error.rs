//! Cloudflare provider error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CloudflareError {
    #[error("wrangler not found. Please install: npm install -g wrangler")]
    WranglerNotFound,

    #[error("wrangler authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("wrangler command failed: {0}")]
    CommandFailed(String),

    #[error("R2 bucket not found: {0}")]
    BucketNotFound(String),

    #[error("Worker not found: {0}")]
    WorkerNotFound(String),

    #[error("DNS record not found: {0}")]
    DnsRecordNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

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

pub type Result<T> = std::result::Result<T, CloudflareError>;
