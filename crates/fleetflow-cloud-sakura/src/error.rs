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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_usacloud_not_found() {
        let err = SakuraError::UsacloudNotFound;
        assert_eq!(
            err.to_string(),
            "usacloud not found. Please install: brew install usacloud"
        );
    }

    #[test]
    fn test_error_display_authentication_failed() {
        let err = SakuraError::AuthenticationFailed("bad token".to_string());
        assert_eq!(err.to_string(), "usacloud authentication failed: bad token");
    }

    #[test]
    fn test_error_display_command_failed() {
        let err = SakuraError::CommandFailed("exit 1".to_string());
        assert_eq!(err.to_string(), "usacloud command failed: exit 1");
    }

    #[test]
    fn test_error_display_server_not_found() {
        let err = SakuraError::ServerNotFound("web-01".to_string());
        assert_eq!(err.to_string(), "Server not found: web-01");
    }

    #[test]
    fn test_error_display_disk_not_found() {
        let err = SakuraError::DiskNotFound("disk-01".to_string());
        assert_eq!(err.to_string(), "Disk not found: disk-01");
    }

    #[test]
    fn test_error_display_invalid_zone() {
        let err = SakuraError::InvalidZone("xx1a".to_string());
        assert_eq!(err.to_string(), "Invalid zone: xx1a");
    }

    #[test]
    fn test_error_display_invalid_plan() {
        let err = SakuraError::InvalidPlan("99core-99gb".to_string());
        assert_eq!(err.to_string(), "Invalid plan: 99core-99gb");
    }

    #[test]
    fn test_error_display_ssh_key_not_found() {
        let err = SakuraError::SshKeyNotFound("deploy-key".to_string());
        assert_eq!(err.to_string(), "SSH key not found: deploy-key");
    }

    #[test]
    fn test_error_display_creation_failed() {
        let err = SakuraError::CreationFailed("quota exceeded".to_string());
        assert_eq!(err.to_string(), "Resource creation failed: quota exceeded");
    }

    #[test]
    fn test_error_display_deletion_failed() {
        let err = SakuraError::DeletionFailed("still in use".to_string());
        assert_eq!(err.to_string(), "Resource deletion failed: still in use");
    }

    #[test]
    fn test_error_from_json() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let sakura_err: SakuraError = json_err.into();
        assert!(sakura_err.to_string().starts_with("JSON parse error:"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let sakura_err: SakuraError = io_err.into();
        assert!(sakura_err.to_string().contains("file missing"));
    }

    #[test]
    fn test_error_from_cloud_error() {
        let cloud_err = fleetflow_cloud::CloudError::ApiError("api failure".to_string());
        let sakura_err: SakuraError = cloud_err.into();
        assert!(sakura_err.to_string().contains("api failure"));
    }
}
