//! Cloudflare provider for FleetFlow
//!
//! This crate implements the CloudProvider trait for Cloudflare,
//! enabling FleetFlow to manage R2 buckets, Workers, and DNS records.
//!
//! # Features
//!
//! - R2 bucket management (create, delete, list)
//! - Worker deployment (planned)
//! - DNS record management via Cloudflare API
//!
//! # Requirements
//!
//! - `wrangler` CLI must be installed and configured (for R2/Workers)
//! - For DNS: `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ZONE_ID`, `CLOUDFLARE_DOMAIN` env vars
//!
//! # Example
//!
//! ```ignore
//! use fleetflow_cloud_cloudflare::CloudflareProvider;
//! use fleetflow_cloud::CloudProvider;
//!
//! let provider = CloudflareProvider::new(Some("account-id".to_string()));
//!
//! // Check authentication
//! let auth = provider.check_auth().await?;
//! if !auth.authenticated {
//!     panic!("Not authenticated: {:?}", auth.error);
//! }
//!
//! // Get current state
//! let state = provider.get_state().await?;
//! ```
//!
//! # DNS Management
//!
//! ```ignore
//! use fleetflow_cloud_cloudflare::dns::{CloudflareDns, DnsConfig};
//!
//! let config = DnsConfig::from_env()?;
//! let dns = CloudflareDns::new(config);
//!
//! // Ensure a DNS record exists
//! let record = dns.ensure_record("mcp-prod", "203.0.113.1").await?;
//!
//! // Remove a DNS record
//! dns.remove_record("mcp-prod").await?;
//! ```

pub mod dns;
pub mod error;
pub mod provider;
pub mod wrangler;

pub use dns::{CloudflareDns, DnsConfig};
pub use error::{CloudflareError, Result};
pub use provider::CloudflareProvider;
pub use wrangler::{DnsRecordInfo, R2BucketInfo, WorkerConfig, WorkerInfo, Wrangler};
