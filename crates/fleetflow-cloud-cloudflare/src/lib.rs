//! Cloudflare provider for FleetFlow
//!
//! This crate implements the CloudProvider trait for Cloudflare,
//! enabling FleetFlow to manage R2 buckets, Workers, and DNS records.
//!
//! # Features
//!
//! - R2 bucket management (create, delete, list)
//! - Worker deployment (planned)
//! - DNS record management (planned)
//!
//! # Requirements
//!
//! - `wrangler` CLI must be installed and configured
//! - Authentication is managed through wrangler login
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

pub mod error;
pub mod provider;
pub mod wrangler;

pub use error::{CloudflareError, Result};
pub use provider::CloudflareProvider;
pub use wrangler::{DnsRecordInfo, R2BucketInfo, WorkerConfig, WorkerInfo, Wrangler};
