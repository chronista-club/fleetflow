//! wrangler CLI wrapper
//!
//! Wraps the wrangler CLI commands for Cloudflare operations.
//! This is a skeleton implementation for future development.

use crate::error::{CloudflareError, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;

/// wrangler CLI wrapper
pub struct Wrangler {
    account_id: Option<String>,
}

impl Wrangler {
    pub fn new(account_id: Option<String>) -> Self {
        Self { account_id }
    }

    /// Check if wrangler is installed and authenticated
    pub async fn check_auth(&self) -> Result<WranglerAuth> {
        // Check if wrangler exists
        let which = Command::new("which").arg("wrangler").output().await?;

        if !which.status.success() {
            return Err(CloudflareError::WranglerNotFound);
        }

        // Check authentication by running whoami
        let output = self.run_command(&["whoami"]).await?;

        // Parse output to get account info
        // Note: wrangler whoami doesn't output JSON by default
        Ok(WranglerAuth {
            authenticated: !output.contains("not authenticated"),
            account_id: self.account_id.clone(),
            email: None, // Would need to parse from output
        })
    }

    /// Run a wrangler command and return stdout
    async fn run_command(&self, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new("wrangler");
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        tracing::debug!("Running: wrangler {}", args.join(" "));

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CloudflareError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    // ========== Pages Operations ==========

    /// Cloudflare Pages にデプロイ
    ///
    /// `wrangler pages deploy <directory> --project-name <project>`
    pub async fn pages_deploy(&self, directory: &str, project_name: &str) -> Result<PagesDeployResult> {
        tracing::info!(
            project = project_name,
            directory = directory,
            "Deploying to Cloudflare Pages"
        );

        let output = self
            .run_command(&["pages", "deploy", directory, "--project-name", project_name])
            .await?;

        // wrangler pages deploy の出力から URL を抽出
        let url = output
            .lines()
            .find(|line| line.contains("https://"))
            .and_then(|line| {
                line.split_whitespace()
                    .find(|word| word.starts_with("https://"))
            })
            .map(String::from);

        Ok(PagesDeployResult {
            project: project_name.to_string(),
            url,
            output,
        })
    }

    // ========== R2 Bucket Operations ==========

    /// List all R2 buckets
    pub async fn list_r2_buckets(&self) -> Result<Vec<R2BucketInfo>> {
        let _output = self.run_command(&["r2", "bucket", "list"]).await?;
        // TODO: Parse output when implementing
        Ok(Vec::new())
    }

    /// Create an R2 bucket
    pub async fn create_r2_bucket(&self, name: &str) -> Result<R2BucketInfo> {
        let _output = self.run_command(&["r2", "bucket", "create", name]).await?;
        // TODO: Parse output when implementing
        Ok(R2BucketInfo {
            name: name.to_string(),
            created_at: None,
        })
    }

    /// Delete an R2 bucket
    pub async fn delete_r2_bucket(&self, name: &str) -> Result<()> {
        self.run_command(&["r2", "bucket", "delete", name]).await?;
        Ok(())
    }

    // ========== Worker Operations ==========

    /// List all Workers
    pub async fn list_workers(&self) -> Result<Vec<WorkerInfo>> {
        // TODO: Implement when needed
        Ok(Vec::new())
    }

    /// Deploy a Worker
    pub async fn deploy_worker(&self, _config: &WorkerConfig) -> Result<WorkerInfo> {
        // TODO: Implement when needed
        Err(CloudflareError::CommandFailed(
            "Worker deployment not yet implemented".to_string(),
        ))
    }

    /// Delete a Worker
    pub async fn delete_worker(&self, name: &str) -> Result<()> {
        self.run_command(&["delete", "--name", name, "--force"])
            .await?;
        Ok(())
    }

    // ========== DNS Operations ==========

    /// List DNS records for a zone
    pub async fn list_dns_records(&self, _zone_id: &str) -> Result<Vec<DnsRecordInfo>> {
        // TODO: Implement when needed
        // Note: wrangler doesn't have direct DNS commands, may need API
        Ok(Vec::new())
    }
}

/// Authentication status from wrangler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WranglerAuth {
    pub authenticated: bool,
    pub account_id: Option<String>,
    pub email: Option<String>,
}

/// R2 bucket information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct R2BucketInfo {
    pub name: String,
    pub created_at: Option<String>,
}

/// Worker information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub name: String,
    pub created_at: Option<String>,
    pub routes: Vec<String>,
}

/// Configuration for deploying a Worker
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub name: String,
    pub script_path: String,
    pub routes: Vec<String>,
    pub vars: std::collections::HashMap<String, String>,
}

/// Cloudflare Pages デプロイ結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagesDeployResult {
    /// プロジェクト名
    pub project: String,
    /// デプロイ先 URL
    pub url: Option<String>,
    /// wrangler の出力
    pub output: String,
}

/// DNS record information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecordInfo {
    pub id: String,
    pub name: String,
    pub record_type: String,
    pub content: String,
    pub ttl: Option<u32>,
    pub proxied: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Wrangler construction ----

    #[test]
    fn test_wrangler_new_with_account_id() {
        let wrangler = Wrangler::new(Some("abc-123".to_string()));
        assert_eq!(wrangler.account_id, Some("abc-123".to_string()));
    }

    #[test]
    fn test_wrangler_new_without_account_id() {
        let wrangler = Wrangler::new(None);
        assert!(wrangler.account_id.is_none());
    }

    // ---- WranglerAuth tests ----

    #[test]
    fn test_wrangler_auth_serde_roundtrip() {
        let auth = WranglerAuth {
            authenticated: true,
            account_id: Some("acc-1".to_string()),
            email: Some("user@example.com".to_string()),
        };

        let json = serde_json::to_string(&auth).unwrap();
        let deserialized: WranglerAuth = serde_json::from_str(&json).unwrap();

        assert!(deserialized.authenticated);
        assert_eq!(deserialized.account_id, Some("acc-1".to_string()));
        assert_eq!(deserialized.email, Some("user@example.com".to_string()));
    }

    #[test]
    fn test_wrangler_auth_unauthenticated() {
        let auth = WranglerAuth {
            authenticated: false,
            account_id: None,
            email: None,
        };

        assert!(!auth.authenticated);
        assert!(auth.account_id.is_none());
        assert!(auth.email.is_none());
    }

    // ---- R2BucketInfo tests ----

    #[test]
    fn test_r2_bucket_info_serde_roundtrip() {
        let bucket = R2BucketInfo {
            name: "my-bucket".to_string(),
            created_at: Some("2025-01-01T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&bucket).unwrap();
        let deserialized: R2BucketInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "my-bucket");
        assert_eq!(
            deserialized.created_at,
            Some("2025-01-01T00:00:00Z".to_string())
        );
    }

    #[test]
    fn test_r2_bucket_info_no_created_at() {
        let bucket = R2BucketInfo {
            name: "new-bucket".to_string(),
            created_at: None,
        };

        assert!(bucket.created_at.is_none());
    }

    // ---- WorkerInfo tests ----

    #[test]
    fn test_worker_info_serde_roundtrip() {
        let worker = WorkerInfo {
            name: "my-worker".to_string(),
            created_at: Some("2025-06-01".to_string()),
            routes: vec!["*.example.com/*".to_string()],
        };

        let json = serde_json::to_string(&worker).unwrap();
        let deserialized: WorkerInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "my-worker");
        assert_eq!(deserialized.routes.len(), 1);
    }

    // ---- WorkerConfig tests ----

    #[test]
    fn test_worker_config_construction() {
        let config = WorkerConfig {
            name: "api-worker".to_string(),
            script_path: "./src/worker.js".to_string(),
            routes: vec!["api.example.com/*".to_string()],
            vars: [("ENV".to_string(), "production".to_string())]
                .into_iter()
                .collect(),
        };

        assert_eq!(config.name, "api-worker");
        assert_eq!(config.script_path, "./src/worker.js");
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.vars.get("ENV"), Some(&"production".to_string()));
    }

    // ---- DnsRecordInfo tests ----

    #[test]
    fn test_dns_record_info_serde_roundtrip() {
        let record = DnsRecordInfo {
            id: "rec-abc".to_string(),
            name: "mcp-prod.example.com".to_string(),
            record_type: "A".to_string(),
            content: "203.0.113.1".to_string(),
            ttl: Some(300),
            proxied: false,
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: DnsRecordInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "rec-abc");
        assert_eq!(deserialized.name, "mcp-prod.example.com");
        assert_eq!(deserialized.record_type, "A");
        assert_eq!(deserialized.content, "203.0.113.1");
        assert_eq!(deserialized.ttl, Some(300));
        assert!(!deserialized.proxied);
    }

    #[test]
    fn test_dns_record_info_cname() {
        let record = DnsRecordInfo {
            id: "rec-cname".to_string(),
            name: "www.example.com".to_string(),
            record_type: "CNAME".to_string(),
            content: "example.com".to_string(),
            ttl: None,
            proxied: true,
        };

        assert_eq!(record.record_type, "CNAME");
        assert!(record.proxied);
        assert!(record.ttl.is_none());
    }
}
