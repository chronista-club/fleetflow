//! AWS provider error types

use thiserror::Error;

/// AWS プロバイダ固有のエラー
#[derive(Error, Debug)]
pub enum AwsError {
    #[error("AWS API error: {0}")]
    ApiError(String),

    #[error("AWS authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Invalid instance type mapping: cpu={cpu}, memory={memory_gb}GB")]
    InvalidInstanceType { cpu: i32, memory_gb: i32 },

    #[error("Invalid CIDR: {0}")]
    InvalidCidr(String),

    #[error("Invalid port: {0}")]
    InvalidPort(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<AwsError> for fleetflow_cloud::CloudError {
    fn from(err: AwsError) -> Self {
        match err {
            AwsError::ApiError(msg) => fleetflow_cloud::CloudError::ApiError(msg),
            AwsError::AuthenticationFailed(msg) => {
                fleetflow_cloud::CloudError::AuthenticationFailed(msg)
            }
            AwsError::ResourceNotFound(msg) => fleetflow_cloud::CloudError::ResourceNotFound(msg),
            AwsError::InvalidConfig(msg) | AwsError::InvalidCidr(msg) => {
                fleetflow_cloud::CloudError::InvalidConfig(msg)
            }
            AwsError::InvalidPort(msg) => fleetflow_cloud::CloudError::InvalidConfig(msg),
            AwsError::InvalidInstanceType { cpu, memory_gb } => {
                fleetflow_cloud::CloudError::InvalidConfig(format!(
                    "No matching instance type for cpu={cpu}, memory={memory_gb}GB"
                ))
            }
        }
    }
}
