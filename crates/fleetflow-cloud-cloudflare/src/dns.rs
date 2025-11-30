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

        Ok(api_response.result.into_iter().next().map(|r| DnsRecordInfo {
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
        let url = format!(
            "{}/zones/{}/dns_records",
            CLOUDFLARE_API_BASE, self.zone_id
        );

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
                tracing::debug!("DNS record already exists with correct IP: {}", existing.name);
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

        tracing::info!("Creating DNS record: {}.{} -> {}", subdomain, self.domain, ip);
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

    #[test]
    fn test_generate_subdomain() {
        let config = DnsConfig {
            api_token: "test".to_string(),
            zone_id: "test".to_string(),
            domain: "example.com".to_string(),
        };
        let dns = CloudflareDns::new(config);

        assert_eq!(dns.generate_subdomain("creo-mcp-server", "prod"), "mcp-prod");
        assert_eq!(dns.generate_subdomain("creo-api-server", "dev"), "api-dev");
        assert_eq!(
            dns.generate_subdomain("creo-memory-viewer", "prod"),
            "memory-prod"
        );
        assert_eq!(dns.generate_subdomain("nginx", "prod"), "nginx-prod");
    }

    #[test]
    fn test_full_domain() {
        let config = DnsConfig {
            api_token: "test".to_string(),
            zone_id: "test".to_string(),
            domain: "example.com".to_string(),
        };
        let dns = CloudflareDns::new(config);

        assert_eq!(dns.full_domain("mcp-prod"), "mcp-prod.example.com");
    }
}
