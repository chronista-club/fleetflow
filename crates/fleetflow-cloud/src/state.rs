//! State management for cloud resources
//!
//! Manages the `.fleetflow/state.json` file which tracks the current state
//! of all cloud resources.

use crate::error::{CloudError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

const STATE_VERSION: u32 = 1;
const STATE_DIR: &str = ".fleetflow";
const STATE_FILE: &str = "state.json";
const STATE_BACKUP: &str = "state.json.backup";
const LOCK_FILE: &str = "lock.json";

/// Global state containing all provider states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalState {
    /// State file version
    pub version: u32,

    /// Last modified timestamp
    pub updated_at: DateTime<Utc>,

    /// Resources indexed by provider:type:id
    pub resources: HashMap<String, ResourceState>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            updated_at: Utc::now(),
            resources: HashMap::new(),
        }
    }
}

impl GlobalState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get resources for a specific provider
    pub fn get_provider_resources(&self, provider: &str) -> Vec<(&String, &ResourceState)> {
        self.resources
            .iter()
            .filter(|(k, _)| k.starts_with(&format!("{}:", provider)))
            .collect()
    }

    /// Add or update a resource
    pub fn set_resource(&mut self, key: String, state: ResourceState) {
        self.resources.insert(key, state);
        self.updated_at = Utc::now();
    }

    /// Remove a resource
    pub fn remove_resource(&mut self, key: &str) -> Option<ResourceState> {
        let result = self.resources.remove(key);
        if result.is_some() {
            self.updated_at = Utc::now();
        }
        result
    }

    /// Get a resource by key
    pub fn get_resource(&self, key: &str) -> Option<&ResourceState> {
        self.resources.get(key)
    }
}

/// State for a single provider
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderState {
    /// Resources managed by this provider
    pub resources: HashMap<String, ResourceState>,
}

impl ProviderState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    pub fn add(&mut self, id: String, state: ResourceState) {
        self.resources.insert(id, state);
    }

    pub fn get(&self, id: &str) -> Option<&ResourceState> {
        self.resources.get(id)
    }

    pub fn remove(&mut self, id: &str) -> Option<ResourceState> {
        self.resources.remove(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &ResourceState)> {
        self.resources.iter()
    }
}

/// State of a single resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceState {
    /// Provider-specific resource ID
    pub id: String,

    /// Resource type
    pub resource_type: String,

    /// Current status
    pub status: ResourceStatus,

    /// Resource attributes (IP, URL, etc.)
    pub attributes: HashMap<String, serde_json::Value>,

    /// When the resource was created
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl ResourceState {
    pub fn new(id: impl Into<String>, resource_type: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            resource_type: resource_type.into(),
            status: ResourceStatus::Unknown,
            attributes: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_status(mut self, status: ResourceStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_attribute(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.attributes.insert(key.into(), value);
        self
    }

    pub fn set_attribute(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.attributes.insert(key.into(), value);
        self.updated_at = Utc::now();
    }

    pub fn get_attribute<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.attributes
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// Status of a resource
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceStatus {
    /// Resource is being created
    Creating,
    /// Resource is running/active
    Running,
    /// Resource is stopped
    Stopped,
    /// Resource is being deleted
    Deleting,
    /// Resource has been deleted
    Deleted,
    /// Resource is in error state
    Error,
    /// Status is unknown
    Unknown,
}

impl std::fmt::Display for ResourceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceStatus::Creating => write!(f, "creating"),
            ResourceStatus::Running => write!(f, "running"),
            ResourceStatus::Stopped => write!(f, "stopped"),
            ResourceStatus::Deleting => write!(f, "deleting"),
            ResourceStatus::Deleted => write!(f, "deleted"),
            ResourceStatus::Error => write!(f, "error"),
            ResourceStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// State manager for reading/writing state files
pub struct StateManager {
    /// Project root directory
    project_root: PathBuf,
}

impl StateManager {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            project_root: project_root.as_ref().to_path_buf(),
        }
    }

    /// Get the state directory path
    fn state_dir(&self) -> PathBuf {
        self.project_root.join(STATE_DIR)
    }

    /// Get the state file path
    fn state_path(&self) -> PathBuf {
        self.state_dir().join(STATE_FILE)
    }

    /// Get the backup file path
    fn backup_path(&self) -> PathBuf {
        self.state_dir().join(STATE_BACKUP)
    }

    /// Get the lock file path
    fn lock_path(&self) -> PathBuf {
        self.state_dir().join(LOCK_FILE)
    }

    /// Ensure the state directory exists
    async fn ensure_state_dir(&self) -> Result<()> {
        let dir = self.state_dir();
        if !dir.exists() {
            fs::create_dir_all(&dir).await?;
            tracing::debug!("Created state directory: {}", dir.display());
        }
        Ok(())
    }

    /// Load the current state
    pub async fn load(&self) -> Result<GlobalState> {
        let path = self.state_path();
        if !path.exists() {
            tracing::debug!("State file not found, returning empty state");
            return Ok(GlobalState::new());
        }

        let content = fs::read_to_string(&path).await?;
        let state: GlobalState = serde_json::from_str(&content)?;

        // Version check
        if state.version > STATE_VERSION {
            return Err(CloudError::StateError(format!(
                "State file version {} is newer than supported version {}",
                state.version, STATE_VERSION
            )));
        }

        tracing::debug!("Loaded state with {} resources", state.resources.len());
        Ok(state)
    }

    /// Save the state
    pub async fn save(&self, state: &GlobalState) -> Result<()> {
        self.ensure_state_dir().await?;

        let path = self.state_path();
        let backup = self.backup_path();

        // Create backup if state file exists
        if path.exists() {
            if backup.exists() {
                fs::remove_file(&backup).await?;
            }
            fs::rename(&path, &backup).await?;
            tracing::debug!("Created state backup");
        }

        // Write new state
        let content = serde_json::to_string_pretty(state)?;
        fs::write(&path, content).await?;

        tracing::debug!("Saved state with {} resources", state.resources.len());
        Ok(())
    }

    /// Acquire a lock for exclusive access
    pub async fn acquire_lock(&self) -> Result<StateLock> {
        self.ensure_state_dir().await?;

        let lock_path = self.lock_path();

        // Check for existing lock
        if lock_path.exists() {
            let content = fs::read_to_string(&lock_path).await?;
            let lock_info: LockInfo = serde_json::from_str(&content)?;

            // Check if lock is stale (older than 1 hour)
            let age = Utc::now().signed_duration_since(lock_info.acquired_at);
            if age.num_hours() < 1 {
                return Err(CloudError::LockError(format!(
                    "State is locked by {} since {}",
                    lock_info.holder, lock_info.acquired_at
                )));
            }

            tracing::warn!("Removing stale lock from {}", lock_info.holder);
        }

        // Create lock
        let lock_info = LockInfo {
            holder: std::env::var("HOSTNAME")
                .or_else(|_| std::env::var("HOST"))
                .unwrap_or_else(|_| "unknown".to_string()),
            acquired_at: Utc::now(),
        };

        let content = serde_json::to_string_pretty(&lock_info)?;
        fs::write(&lock_path, content).await?;

        tracing::debug!("Acquired state lock");
        Ok(StateLock {
            lock_path,
            released: false,
        })
    }
}

/// Lock information
#[derive(Debug, Serialize, Deserialize)]
struct LockInfo {
    holder: String,
    acquired_at: DateTime<Utc>,
}

/// RAII guard for state lock
pub struct StateLock {
    lock_path: PathBuf,
    released: bool,
}

impl StateLock {
    /// Release the lock
    pub async fn release(mut self) -> Result<()> {
        if !self.released {
            if self.lock_path.exists() {
                fs::remove_file(&self.lock_path).await?;
                tracing::debug!("Released state lock");
            }
            self.released = true;
        }
        Ok(())
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        if !self.released && self.lock_path.exists() {
            // Synchronous cleanup in drop - not ideal but necessary
            let _ = std::fs::remove_file(&self.lock_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_state_save_load() {
        let temp_dir = tempdir().unwrap();
        let manager = StateManager::new(temp_dir.path());

        let mut state = GlobalState::new();
        state.set_resource(
            "sakura-cloud:server:test-01".to_string(),
            ResourceState::new("123456", "server")
                .with_status(ResourceStatus::Running)
                .with_attribute("ip", serde_json::json!("192.168.1.1")),
        );

        manager.save(&state).await.unwrap();

        let loaded = manager.load().await.unwrap();
        assert_eq!(loaded.resources.len(), 1);
        assert!(loaded.resources.contains_key("sakura-cloud:server:test-01"));
    }

    #[tokio::test]
    async fn test_empty_state() {
        let temp_dir = tempdir().unwrap();
        let manager = StateManager::new(temp_dir.path());

        let state = manager.load().await.unwrap();
        assert!(state.resources.is_empty());
    }

    #[tokio::test]
    async fn test_state_backup_created() {
        let temp_dir = tempdir().unwrap();
        let manager = StateManager::new(temp_dir.path());

        // Save twice to trigger backup
        let state = GlobalState::new();
        manager.save(&state).await.unwrap();

        let mut state2 = GlobalState::new();
        state2.set_resource("test:key".to_string(), ResourceState::new("1", "server"));
        manager.save(&state2).await.unwrap();

        // Backup should exist
        let backup_path = temp_dir.path().join(".fleetflow").join("state.json.backup");
        assert!(backup_path.exists());
    }

    // ---- GlobalState tests ----

    #[test]
    fn test_global_state_default() {
        let state = GlobalState::new();
        assert_eq!(state.version, 1);
        assert!(state.resources.is_empty());
    }

    #[test]
    fn test_global_state_set_and_get_resource() {
        let mut state = GlobalState::new();
        let resource = ResourceState::new("123", "server").with_status(ResourceStatus::Running);
        state.set_resource("sakura:server:web".to_string(), resource);

        let found = state.get_resource("sakura:server:web");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "123");
        assert_eq!(found.unwrap().status, ResourceStatus::Running);
    }

    #[test]
    fn test_global_state_remove_resource() {
        let mut state = GlobalState::new();
        state.set_resource("key1".to_string(), ResourceState::new("1", "server"));

        let removed = state.remove_resource("key1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "1");
        assert!(state.get_resource("key1").is_none());
    }

    #[test]
    fn test_global_state_remove_nonexistent() {
        let mut state = GlobalState::new();
        let removed = state.remove_resource("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_global_state_get_provider_resources() {
        let mut state = GlobalState::new();
        state.set_resource(
            "sakura:server:web".to_string(),
            ResourceState::new("1", "server"),
        );
        state.set_resource(
            "sakura:server:db".to_string(),
            ResourceState::new("2", "server"),
        );
        state.set_resource(
            "cloudflare:r2:bucket1".to_string(),
            ResourceState::new("3", "r2-bucket"),
        );

        let sakura_resources = state.get_provider_resources("sakura");
        assert_eq!(sakura_resources.len(), 2);

        let cf_resources = state.get_provider_resources("cloudflare");
        assert_eq!(cf_resources.len(), 1);

        let empty = state.get_provider_resources("aws");
        assert!(empty.is_empty());
    }

    // ---- ProviderState tests ----

    #[test]
    fn test_provider_state_is_empty() {
        let mut state = ProviderState::new();
        assert!(state.is_empty());

        state.add("web".to_string(), ResourceState::new("1", "server"));
        assert!(!state.is_empty());

        state.remove("web");
        assert!(state.is_empty());
    }

    #[test]
    fn test_provider_state_crud() {
        let mut state = ProviderState::new();
        assert!(state.resources.is_empty());

        state.add("web".to_string(), ResourceState::new("1", "server"));
        assert!(state.get("web").is_some());

        let removed = state.remove("web");
        assert!(removed.is_some());
        assert!(state.get("web").is_none());
    }

    #[test]
    fn test_provider_state_iter() {
        let mut state = ProviderState::new();
        state.add("a".to_string(), ResourceState::new("1", "server"));
        state.add("b".to_string(), ResourceState::new("2", "server"));

        let items: Vec<_> = state.iter().collect();
        assert_eq!(items.len(), 2);
    }

    // ---- ResourceState tests ----

    #[test]
    fn test_resource_state_new() {
        let state = ResourceState::new("abc", "server");
        assert_eq!(state.id, "abc");
        assert_eq!(state.resource_type, "server");
        assert_eq!(state.status, ResourceStatus::Unknown);
        assert!(state.attributes.is_empty());
    }

    #[test]
    fn test_resource_state_builder() {
        let state = ResourceState::new("1", "server")
            .with_status(ResourceStatus::Running)
            .with_attribute("ip", serde_json::json!("10.0.0.1"))
            .with_attribute("cpu", serde_json::json!(4));

        assert_eq!(state.status, ResourceStatus::Running);
        assert_eq!(state.attributes.len(), 2);
    }

    #[test]
    fn test_resource_state_set_attribute() {
        let mut state = ResourceState::new("1", "server");
        state.set_attribute("ip", serde_json::json!("10.0.0.1"));

        assert_eq!(
            state.get_attribute::<String>("ip"),
            Some("10.0.0.1".to_string())
        );
    }

    #[test]
    fn test_resource_state_get_attribute_typed() {
        let state = ResourceState::new("1", "server")
            .with_attribute("count", serde_json::json!(42))
            .with_attribute("name", serde_json::json!("web"))
            .with_attribute("active", serde_json::json!(true));

        assert_eq!(state.get_attribute::<i32>("count"), Some(42));
        assert_eq!(
            state.get_attribute::<String>("name"),
            Some("web".to_string())
        );
        assert_eq!(state.get_attribute::<bool>("active"), Some(true));
    }

    #[test]
    fn test_resource_state_get_attribute_missing() {
        let state = ResourceState::new("1", "server");
        assert_eq!(state.get_attribute::<String>("nonexistent"), None);
    }

    #[test]
    fn test_resource_state_get_attribute_type_mismatch() {
        let state = ResourceState::new("1", "server")
            .with_attribute("count", serde_json::json!("not-a-number"));
        assert_eq!(state.get_attribute::<i32>("count"), None);
    }

    // ---- ResourceStatus tests ----

    #[test]
    fn test_resource_status_display() {
        assert_eq!(ResourceStatus::Creating.to_string(), "creating");
        assert_eq!(ResourceStatus::Running.to_string(), "running");
        assert_eq!(ResourceStatus::Stopped.to_string(), "stopped");
        assert_eq!(ResourceStatus::Deleting.to_string(), "deleting");
        assert_eq!(ResourceStatus::Deleted.to_string(), "deleted");
        assert_eq!(ResourceStatus::Error.to_string(), "error");
        assert_eq!(ResourceStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_resource_status_equality() {
        assert_eq!(ResourceStatus::Running, ResourceStatus::Running);
        assert_ne!(ResourceStatus::Running, ResourceStatus::Stopped);
    }

    #[test]
    fn test_resource_status_serde_roundtrip() {
        let cases = vec![
            (ResourceStatus::Creating, "\"creating\""),
            (ResourceStatus::Running, "\"running\""),
            (ResourceStatus::Stopped, "\"stopped\""),
            (ResourceStatus::Deleting, "\"deleting\""),
            (ResourceStatus::Deleted, "\"deleted\""),
            (ResourceStatus::Error, "\"error\""),
            (ResourceStatus::Unknown, "\"unknown\""),
        ];
        for (status, expected_json) in cases {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, expected_json);
            let deserialized: ResourceStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, status);
        }
    }

    // ---- StateManager path tests ----

    #[test]
    fn test_state_manager_paths() {
        let manager = StateManager::new("/project");
        assert_eq!(
            manager.state_dir(),
            std::path::PathBuf::from("/project/.fleetflow")
        );
        assert_eq!(
            manager.state_path(),
            std::path::PathBuf::from("/project/.fleetflow/state.json")
        );
        assert_eq!(
            manager.backup_path(),
            std::path::PathBuf::from("/project/.fleetflow/state.json.backup")
        );
        assert_eq!(
            manager.lock_path(),
            std::path::PathBuf::from("/project/.fleetflow/lock.json")
        );
    }

    // ---- GlobalState serde roundtrip ----

    #[test]
    fn test_global_state_serde_roundtrip() {
        let mut state = GlobalState::new();
        state.set_resource(
            "sakura:server:web".to_string(),
            ResourceState::new("123", "server").with_status(ResourceStatus::Running),
        );

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: GlobalState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.resources.len(), 1);
        let resource = deserialized.get_resource("sakura:server:web").unwrap();
        assert_eq!(resource.id, "123");
        assert_eq!(resource.status, ResourceStatus::Running);
    }
}
