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

    #[error("Missing environment variable: {0}")]
    MissingEnvVar(String),

    #[error("Cloudflare API error: {0}")]
    ApiError(String),

    #[error("HTTP request error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Cloud error: {0}")]
    CloudError(#[from] fleetflow_cloud::CloudError),
}

pub type Result<T> = std::result::Result<T, CloudflareError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_wrangler_not_found() {
        let err = CloudflareError::WranglerNotFound;
        assert_eq!(
            err.to_string(),
            "wrangler not found. Please install: npm install -g wrangler"
        );
    }

    #[test]
    fn test_error_display_authentication_failed() {
        let err = CloudflareError::AuthenticationFailed("bad token".to_string());
        assert_eq!(err.to_string(), "wrangler authentication failed: bad token");
    }

    #[test]
    fn test_error_display_command_failed() {
        let err = CloudflareError::CommandFailed("exit 1".to_string());
        assert_eq!(err.to_string(), "wrangler command failed: exit 1");
    }

    #[test]
    fn test_error_display_bucket_not_found() {
        let err = CloudflareError::BucketNotFound("my-bucket".to_string());
        assert_eq!(err.to_string(), "R2 bucket not found: my-bucket");
    }

    #[test]
    fn test_error_display_worker_not_found() {
        let err = CloudflareError::WorkerNotFound("my-worker".to_string());
        assert_eq!(err.to_string(), "Worker not found: my-worker");
    }

    #[test]
    fn test_error_display_dns_record_not_found() {
        let err = CloudflareError::DnsRecordNotFound("mcp-prod".to_string());
        assert_eq!(err.to_string(), "DNS record not found: mcp-prod");
    }

    #[test]
    fn test_error_display_invalid_config() {
        let err = CloudflareError::InvalidConfig("missing field".to_string());
        assert_eq!(err.to_string(), "Invalid configuration: missing field");
    }

    #[test]
    fn test_error_display_creation_failed() {
        let err = CloudflareError::CreationFailed("quota".to_string());
        assert_eq!(err.to_string(), "Resource creation failed: quota");
    }

    #[test]
    fn test_error_display_deletion_failed() {
        let err = CloudflareError::DeletionFailed("in use".to_string());
        assert_eq!(err.to_string(), "Resource deletion failed: in use");
    }

    #[test]
    fn test_error_display_missing_env_var() {
        let err = CloudflareError::MissingEnvVar("CLOUDFLARE_API_TOKEN".to_string());
        assert_eq!(
            err.to_string(),
            "Missing environment variable: CLOUDFLARE_API_TOKEN"
        );
    }

    #[test]
    fn test_error_display_api_error() {
        let err = CloudflareError::ApiError("rate limited".to_string());
        assert_eq!(err.to_string(), "Cloudflare API error: rate limited");
    }

    #[test]
    fn test_error_from_json() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let cf_err: CloudflareError = json_err.into();
        assert!(cf_err.to_string().starts_with("JSON parse error:"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let cf_err: CloudflareError = io_err.into();
        assert!(cf_err.to_string().contains("file missing"));
    }

    #[test]
    fn test_error_from_cloud_error() {
        let cloud_err = fleetflow_cloud::CloudError::ApiError("upstream".to_string());
        let cf_err: CloudflareError = cloud_err.into();
        assert!(cf_err.to_string().contains("upstream"));
    }
}
