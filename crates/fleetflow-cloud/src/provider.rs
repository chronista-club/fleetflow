//! Cloud provider trait definition

use crate::action::{ApplyResult, Plan};
use crate::error::Result;
use crate::state::ProviderState;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Cloud provider abstraction trait
///
/// All cloud providers (Sakura Cloud, Cloudflare, etc.) implement this trait
/// to provide a unified interface for resource management.
#[async_trait]
pub trait CloudProvider: Send + Sync {
    /// Returns the provider name (e.g., "sakura-cloud", "cloudflare")
    fn name(&self) -> &str;

    /// Returns the provider display name for UI
    fn display_name(&self) -> &str;

    /// Check if the provider is properly configured and authenticated
    async fn check_auth(&self) -> Result<AuthStatus>;

    /// Get the current state of all resources managed by this provider
    async fn get_state(&self) -> Result<ProviderState>;

    /// Calculate the diff between desired and current state
    async fn plan(&self, desired: &ResourceSet) -> Result<Plan>;

    /// Apply the planned actions
    async fn apply(&self, plan: &Plan) -> Result<ApplyResult>;

    /// Destroy a specific resource
    async fn destroy(&self, resource_id: &str) -> Result<()>;

    /// Destroy all resources managed by this provider
    async fn destroy_all(&self) -> Result<ApplyResult>;
}

/// Authentication status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    /// Whether authentication is valid
    pub authenticated: bool,

    /// Account/user information if available
    pub account_info: Option<String>,

    /// Error message if not authenticated
    pub error: Option<String>,
}

impl AuthStatus {
    pub fn ok(account_info: impl Into<String>) -> Self {
        Self {
            authenticated: true,
            account_info: Some(account_info.into()),
            error: None,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            authenticated: false,
            account_info: None,
            error: Some(error.into()),
        }
    }
}

/// Set of resources to be managed
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceSet {
    /// Resources indexed by type and ID
    pub resources: HashMap<String, ResourceConfig>,
}

impl ResourceSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, resource: ResourceConfig) {
        let key = format!("{}:{}", resource.resource_type, resource.id);
        self.resources.insert(key, resource);
    }

    pub fn get(&self, resource_type: &str, id: &str) -> Option<&ResourceConfig> {
        let key = format!("{}:{}", resource_type, id);
        self.resources.get(&key)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ResourceConfig> {
        self.resources.values()
    }

    pub fn by_type(&self, resource_type: &str) -> Vec<&ResourceConfig> {
        self.resources
            .values()
            .filter(|r| r.resource_type == resource_type)
            .collect()
    }
}

/// Configuration for a cloud resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    /// Resource type (e.g., "server", "r2-bucket")
    pub resource_type: String,

    /// Resource identifier
    pub id: String,

    /// Provider name
    pub provider: String,

    /// Resource-specific configuration
    pub config: serde_json::Value,
}

impl ResourceConfig {
    pub fn new(
        resource_type: impl Into<String>,
        id: impl Into<String>,
        provider: impl Into<String>,
        config: serde_json::Value,
    ) -> Self {
        Self {
            resource_type: resource_type.into(),
            id: id.into(),
            provider: provider.into(),
            config,
        }
    }

    /// Get the full resource key (type:id)
    pub fn key(&self) -> String {
        format!("{}:{}", self.resource_type, self.id)
    }

    /// Get a configuration value as a specific type
    pub fn get_config<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.config.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// Retry configuration for provider operations
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,

    /// Initial delay between retries
    pub initial_delay: std::time::Duration,

    /// Maximum delay between retries
    pub max_delay: std::time::Duration,

    /// Backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: std::time::Duration::from_secs(1),
            max_delay: std::time::Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}
