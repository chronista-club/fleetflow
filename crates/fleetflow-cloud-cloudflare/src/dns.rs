//! Cloudflare DNS API client
//!
//! Direct Cloudflare API implementation for DNS record management.
//! Uses Bearer token authentication instead of wrangler CLI.

use crate::error::{CloudflareError, Result};
use crate::wrangler::DnsRecordInfo;
use serde::{Deserialize, Serialize};

const CLOUDFLARE_API_BASE: &str = "https://api.cloudflare.com/client/v4";

/// Cloudflare DNS manager
pub struct CloudflareDns {
    client: reqwest::Client,
    api_token: String,
    zone_id: String,
    domain: String,
}

/// Configuration for DNS manager
#[derive(Debug, Clone)]
pub struct DnsConfig {
    pub api_token: String,
    pub zone_id: String,
    pub domain: String,
}

impl DnsConfig {
    /// Create DnsConfig from environment variables
    pub fn from_env() -> Result<Self> {
        let api_token = std::env::var("CLOUDFLARE_API_TOKEN")
            .map_err(|_| CloudflareError::MissingEnvVar("CLOUDFLARE_API_TOKEN".to_string()))?;
        let zone_id = std::env::var("CLOUDFLARE_ZONE_ID")
            .map_err(|_| CloudflareError::MissingEnvVar("CLOUDFLARE_ZONE_ID".to_string()))?;
        let domain = std::env::var("CLOUDFLARE_DOMAIN")
            .map_err(|_| CloudflareError::MissingEnvVar("CLOUDFLARE_DOMAIN".to_string()))?;

        Ok(Self {
            api_token,
            zone_id,
            domain,
        })
    }
}

impl CloudflareDns {
    /// Create a new DNS manager
    pub fn new(config: DnsConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_token: config.api_token,
            zone_id: config.zone_id,
            domain: config.domain,
        }
    }

    /// Get the domain
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Generate a subdomain name from service and stage
    pub fn generate_subdomain(&self, service: &str, stage: &str) -> String {
        let short_name = service
            .trim_start_matches("creo-")
            .trim_end_matches("-server")
            .trim_end_matches("-viewer");
        format!("{}-{}", short_name, stage)
    }

    /// Get the full domain name for a subdomain
    pub fn full_domain(&self, subdomain: &str) -> String {
        format!("{}.{}", subdomain, self.domain)
    }

    /// List all DNS A records in the zone
    pub async fn list_records(&self) -> Result<Vec<DnsRecordInfo>> {
        let url = format!(
            "{}/zones/{}/dns_records?type=A",
            CLOUDFLARE_API_BASE, self.zone_id
        );

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await?;

        let api_response: ApiResponse<Vec<ApiDnsRecord>> = response.json().await?;

        if !api_response.success {
            let error_msg = api_response
                .errors
                .first()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(CloudflareError::ApiError(error_msg));
        }

        Ok(api_response
            .result
            .into_iter()
            .map(|r| DnsRecordInfo {
                id: r.id,
                name: r.name,
                record_type: r.r#type,
                content: r.content,
                ttl: Some(r.ttl),
                proxied: r.proxied,
            })
            .collect())
    }

    /// Find a DNS record by subdomain
    pub async fn find_record(&self, subdomain: &str) -> Result<Option<DnsRecordInfo>> {
        let full_name = self.full_domain(subdomain);
        let url = format!(
            "{}/zones/{}/dns_records?type=A&name={}",
            CLOUDFLARE_API_BASE, self.zone_id, full_name
        );

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await?;

        let api_response: ApiResponse<Vec<ApiDnsRecord>> = response.json().await?;

        if !api_response.success {
            let error_msg = api_response
                .errors
                .first()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(CloudflareError::ApiError(error_msg));
        }

        Ok(api_response
            .result
            .into_iter()
            .next()
            .map(|r| DnsRecordInfo {
                id: r.id,
                name: r.name,
                record_type: r.r#type,
                content: r.content,
                ttl: Some(r.ttl),
                proxied: r.proxied,
            }))
    }

    /// Create a new DNS A record
    pub async fn create_record(&self, subdomain: &str, ip: &str) -> Result<DnsRecordInfo> {
        let url = format!("{}/zones/{}/dns_records", CLOUDFLARE_API_BASE, self.zone_id);

        let request_body = CreateDnsRecordRequest {
            r#type: "A".to_string(),
            name: subdomain.to_string(),
            content: ip.to_string(),
            ttl: 1, // Auto
            proxied: false,
        };

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&request_body)
            .send()
            .await?;

        let api_response: ApiResponse<ApiDnsRecord> = response.json().await?;

        if !api_response.success {
            let error_msg = api_response
                .errors
                .first()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(CloudflareError::ApiError(error_msg));
        }

        let r = api_response.result;
        Ok(DnsRecordInfo {
            id: r.id,
            name: r.name,
            record_type: r.r#type,
            content: r.content,
            ttl: Some(r.ttl),
            proxied: r.proxied,
        })
    }

    /// Update an existing DNS record
    pub async fn update_record(&self, record_id: &str, ip: &str) -> Result<DnsRecordInfo> {
        let url = format!(
            "{}/zones/{}/dns_records/{}",
            CLOUDFLARE_API_BASE, self.zone_id, record_id
        );

        let request_body = UpdateDnsRecordRequest {
            content: ip.to_string(),
        };

        let response = self
            .client
            .patch(&url)
            .bearer_auth(&self.api_token)
            .json(&request_body)
            .send()
            .await?;

        let api_response: ApiResponse<ApiDnsRecord> = response.json().await?;

        if !api_response.success {
            let error_msg = api_response
                .errors
                .first()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(CloudflareError::ApiError(error_msg));
        }

        let r = api_response.result;
        Ok(DnsRecordInfo {
            id: r.id,
            name: r.name,
            record_type: r.r#type,
            content: r.content,
            ttl: Some(r.ttl),
            proxied: r.proxied,
        })
    }

    /// Delete a DNS record
    pub async fn delete_record(&self, record_id: &str) -> Result<()> {
        let url = format!(
            "{}/zones/{}/dns_records/{}",
            CLOUDFLARE_API_BASE, self.zone_id, record_id
        );

        let response = self
            .client
            .delete(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await?;

        let api_response: ApiResponse<DeleteResult> = response.json().await?;

        if !api_response.success {
            let error_msg = api_response
                .errors
                .first()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(CloudflareError::ApiError(error_msg));
        }

        Ok(())
    }

    /// Ensure a DNS record exists with the specified IP (create or update)
    pub async fn ensure_record(&self, subdomain: &str, ip: &str) -> Result<DnsRecordInfo> {
        if let Some(existing) = self.find_record(subdomain).await? {
            if existing.content == ip {
                tracing::debug!(
                    "DNS record already exists with correct IP: {}",
                    existing.name
                );
                return Ok(existing);
            }
            tracing::info!(
                "Updating DNS record {} from {} to {}",
                existing.name,
                existing.content,
                ip
            );
            return self.update_record(&existing.id, ip).await;
        }

        tracing::info!(
            "Creating DNS record: {}.{} -> {}",
            subdomain,
            self.domain,
            ip
        );
        self.create_record(subdomain, ip).await
    }

    /// Remove a DNS record if it exists
    pub async fn remove_record(&self, subdomain: &str) -> Result<()> {
        if let Some(record) = self.find_record(subdomain).await? {
            tracing::info!("Deleting DNS record: {}", record.name);
            self.delete_record(&record.id).await?;
        } else {
            tracing::debug!("DNS record not found, nothing to delete: {}", subdomain);
        }
        Ok(())
    }

    // ============ CNAME Record Management ============

    /// Find a CNAME record by subdomain
    pub async fn find_cname_record(&self, subdomain: &str) -> Result<Option<DnsRecordInfo>> {
        let full_name = self.full_domain(subdomain);
        let url = format!(
            "{}/zones/{}/dns_records?type=CNAME&name={}",
            CLOUDFLARE_API_BASE, self.zone_id, full_name
        );

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await?;

        let api_response: ApiResponse<Vec<ApiDnsRecord>> = response.json().await?;

        if !api_response.success {
            let error_msg = api_response
                .errors
                .first()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(CloudflareError::ApiError(error_msg));
        }

        Ok(api_response
            .result
            .into_iter()
            .next()
            .map(|r| DnsRecordInfo {
                id: r.id,
                name: r.name,
                record_type: r.r#type,
                content: r.content,
                ttl: Some(r.ttl),
                proxied: r.proxied,
            }))
    }

    /// Create a new CNAME record
    /// `target` should be a full domain name (e.g., "server-prod.example.com")
    pub async fn create_cname_record(
        &self,
        subdomain: &str,
        target: &str,
    ) -> Result<DnsRecordInfo> {
        let url = format!("{}/zones/{}/dns_records", CLOUDFLARE_API_BASE, self.zone_id);

        let request_body = CreateDnsRecordRequest {
            r#type: "CNAME".to_string(),
            name: subdomain.to_string(),
            content: target.to_string(),
            ttl: 1, // Auto
            proxied: false,
        };

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&request_body)
            .send()
            .await?;

        let api_response: ApiResponse<ApiDnsRecord> = response.json().await?;

        if !api_response.success {
            let error_msg = api_response
                .errors
                .first()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(CloudflareError::ApiError(error_msg));
        }

        let r = api_response.result;
        Ok(DnsRecordInfo {
            id: r.id,
            name: r.name,
            record_type: r.r#type,
            content: r.content,
            ttl: Some(r.ttl),
            proxied: r.proxied,
        })
    }

    /// Update an existing CNAME record
    pub async fn update_cname_record(
        &self,
        record_id: &str,
        target: &str,
    ) -> Result<DnsRecordInfo> {
        let url = format!(
            "{}/zones/{}/dns_records/{}",
            CLOUDFLARE_API_BASE, self.zone_id, record_id
        );

        let request_body = UpdateDnsRecordRequest {
            content: target.to_string(),
        };

        let response = self
            .client
            .patch(&url)
            .bearer_auth(&self.api_token)
            .json(&request_body)
            .send()
            .await?;

        let api_response: ApiResponse<ApiDnsRecord> = response.json().await?;

        if !api_response.success {
            let error_msg = api_response
                .errors
                .first()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(CloudflareError::ApiError(error_msg));
        }

        let r = api_response.result;
        Ok(DnsRecordInfo {
            id: r.id,
            name: r.name,
            record_type: r.r#type,
            content: r.content,
            ttl: Some(r.ttl),
            proxied: r.proxied,
        })
    }

    /// Ensure a CNAME record exists with the specified target (create or update)
    /// `target` should be a full domain name (e.g., "server-prod.example.com")
    pub async fn ensure_cname_record(
        &self,
        subdomain: &str,
        target: &str,
    ) -> Result<DnsRecordInfo> {
        if let Some(existing) = self.find_cname_record(subdomain).await? {
            if existing.content == target {
                tracing::debug!(
                    "CNAME record already exists with correct target: {} -> {}",
                    existing.name,
                    target
                );
                return Ok(existing);
            }
            tracing::info!(
                "Updating CNAME record {} from {} to {}",
                existing.name,
                existing.content,
                target
            );
            return self.update_cname_record(&existing.id, target).await;
        }

        tracing::info!(
            "Creating CNAME record: {}.{} -> {}",
            subdomain,
            self.domain,
            target
        );
        self.create_cname_record(subdomain, target).await
    }

    /// Remove a CNAME record if it exists
    pub async fn remove_cname_record(&self, subdomain: &str) -> Result<()> {
        if let Some(record) = self.find_cname_record(subdomain).await? {
            tracing::info!("Deleting CNAME record: {}", record.name);
            self.delete_record(&record.id).await?;
        } else {
            tracing::debug!("CNAME record not found, nothing to delete: {}", subdomain);
        }
        Ok(())
    }
}

// ============ API Types ============

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    result: T,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[allow(dead_code)]
    code: i32,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ApiDnsRecord {
    id: String,
    name: String,
    #[serde(rename = "type")]
    r#type: String,
    content: String,
    ttl: u32,
    proxied: bool,
}

#[derive(Debug, Serialize)]
struct CreateDnsRecordRequest {
    #[serde(rename = "type")]
    r#type: String,
    name: String,
    content: String,
    ttl: u32,
    proxied: bool,
}

#[derive(Debug, Serialize)]
struct UpdateDnsRecordRequest {
    content: String,
}

#[derive(Debug, Deserialize)]
struct DeleteResult {
    #[allow(dead_code)]
    id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dns(domain: &str) -> CloudflareDns {
        let config = DnsConfig {
            api_token: "test-token".to_string(),
            zone_id: "test-zone".to_string(),
            domain: domain.to_string(),
        };
        CloudflareDns::new(config)
    }

    // ---- generate_subdomain tests ----

    #[test]
    fn test_generate_subdomain() {
        let dns = test_dns("example.com");

        assert_eq!(
            dns.generate_subdomain("creo-mcp-server", "prod"),
            "mcp-prod"
        );
        assert_eq!(dns.generate_subdomain("creo-api-server", "dev"), "api-dev");
        assert_eq!(
            dns.generate_subdomain("creo-memory-viewer", "prod"),
            "memory-prod"
        );
        assert_eq!(dns.generate_subdomain("nginx", "prod"), "nginx-prod");
    }

    #[test]
    fn test_generate_subdomain_no_creo_prefix() {
        let dns = test_dns("example.com");
        assert_eq!(dns.generate_subdomain("web-server", "live"), "web-live");
    }

    #[test]
    fn test_generate_subdomain_no_suffix() {
        let dns = test_dns("example.com");
        assert_eq!(dns.generate_subdomain("creo-api", "dev"), "api-dev");
    }

    #[test]
    fn test_generate_subdomain_both_prefix_and_suffix() {
        let dns = test_dns("example.com");
        // creo- is stripped from start, -server is stripped from end
        assert_eq!(
            dns.generate_subdomain("creo-auth-server", "prod"),
            "auth-prod"
        );
    }

    // ---- full_domain tests ----

    #[test]
    fn test_full_domain() {
        let dns = test_dns("example.com");
        assert_eq!(dns.full_domain("mcp-prod"), "mcp-prod.example.com");
    }

    #[test]
    fn test_full_domain_different_base() {
        let dns = test_dns("myapp.dev");
        assert_eq!(dns.full_domain("api-staging"), "api-staging.myapp.dev");
    }

    // ---- domain accessor test ----

    #[test]
    fn test_domain_accessor() {
        let dns = test_dns("example.com");
        assert_eq!(dns.domain(), "example.com");
    }

    // ---- DnsConfig tests ----

    #[test]
    fn test_dns_config_from_env_missing_token() {
        // Ensure env vars are not set
        // SAFETY: This test runs in a single-threaded context; no other thread
        // relies on these environment variables concurrently.
        unsafe {
            std::env::remove_var("CLOUDFLARE_API_TOKEN");
            std::env::remove_var("CLOUDFLARE_ZONE_ID");
            std::env::remove_var("CLOUDFLARE_DOMAIN");
        }

        let result = DnsConfig::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("CLOUDFLARE_API_TOKEN"));
    }

    // ---- API types serde tests ----

    #[test]
    fn test_create_dns_record_request_serialize() {
        let req = CreateDnsRecordRequest {
            r#type: "A".to_string(),
            name: "mcp-prod".to_string(),
            content: "203.0.113.1".to_string(),
            ttl: 1,
            proxied: false,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["type"], "A");
        assert_eq!(json["name"], "mcp-prod");
        assert_eq!(json["content"], "203.0.113.1");
        assert_eq!(json["ttl"], 1);
        assert_eq!(json["proxied"], false);
    }

    #[test]
    fn test_update_dns_record_request_serialize() {
        let req = UpdateDnsRecordRequest {
            content: "10.0.0.1".to_string(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["content"], "10.0.0.1");
    }

    #[test]
    fn test_api_dns_record_deserialize() {
        let json = r#"{
            "id": "rec-123",
            "name": "mcp-prod.example.com",
            "type": "A",
            "content": "203.0.113.1",
            "ttl": 300,
            "proxied": false
        }"#;

        let record: ApiDnsRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.id, "rec-123");
        assert_eq!(record.name, "mcp-prod.example.com");
        assert_eq!(record.r#type, "A");
        assert_eq!(record.content, "203.0.113.1");
        assert_eq!(record.ttl, 300);
        assert!(!record.proxied);
    }

    #[test]
    fn test_api_response_success() {
        let json = r#"{
            "success": true,
            "result": [
                {
                    "id": "rec-1",
                    "name": "a.example.com",
                    "type": "A",
                    "content": "1.2.3.4",
                    "ttl": 1,
                    "proxied": true
                }
            ],
            "errors": []
        }"#;

        let response: ApiResponse<Vec<ApiDnsRecord>> = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.result.len(), 1);
        assert!(response.errors.is_empty());
    }

    #[test]
    fn test_api_response_failure() {
        let json = r#"{
            "success": false,
            "result": [],
            "errors": [{"code": 1003, "message": "Invalid zone ID"}]
        }"#;

        let response: ApiResponse<Vec<ApiDnsRecord>> = serde_json::from_str(json).unwrap();
        assert!(!response.success);
        assert_eq!(response.errors.len(), 1);
        assert_eq!(response.errors[0].message, "Invalid zone ID");
        assert_eq!(response.errors[0].code, 1003);
    }

    #[test]
    fn test_delete_result_deserialize() {
        let json = r#"{"id": "rec-456"}"#;
        let result: DeleteResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.id, "rec-456");
    }
}
