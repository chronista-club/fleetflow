//! usacloud CLI wrapper
//!
//! Wraps the usacloud CLI commands for Sakura Cloud operations.

use crate::error::{Result, SakuraError};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;

/// usacloud CLI wrapper
pub struct Usacloud {
    zone: String,
}

impl Usacloud {
    pub fn new(zone: impl Into<String>) -> Self {
        Self { zone: zone.into() }
    }

    /// Check if usacloud is installed and authenticated
    pub async fn check_auth(&self) -> Result<UsacloudAuth> {
        // Check if usacloud exists
        let which = Command::new("which")
            .arg("usacloud")
            .output()
            .await?;

        if !which.status.success() {
            return Err(SakuraError::UsacloudNotFound);
        }

        // Check authentication by running a simple command (no zone needed)
        let output = self
            .run_command_global(&["auth-status", "--output-type", "json"])
            .await?;

        let auth: UsacloudAuth = serde_json::from_str(&output)?;
        Ok(auth)
    }

    /// Run a usacloud command without zone (for global commands like auth-status)
    async fn run_command_global(&self, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new("usacloud");
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        tracing::debug!("Running: usacloud {}", args.join(" "));

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SakuraError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Run a usacloud command with zone and return stdout
    /// Zone flag is added after the subcommand: usacloud server list --zone tk1a
    async fn run_command(&self, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new("usacloud");
        cmd.args(args);
        cmd.arg("--zone").arg(&self.zone);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        tracing::debug!("Running: usacloud {} --zone {}", args.join(" "), self.zone);

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SakuraError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// List all servers
    pub async fn list_servers(&self) -> Result<Vec<ServerInfo>> {
        let output = self
            .run_command(&["server", "list", "--output-type", "json"])
            .await?;

        if output.trim().is_empty() || output.trim() == "[]" {
            return Ok(Vec::new());
        }

        let servers: Vec<ServerInfo> = serde_json::from_str(&output)?;
        Ok(servers)
    }

    /// Find servers by tag
    /// Tag format: "key=value" (e.g., "fleetflow:server=creo-vps")
    pub async fn find_servers_by_tag(&self, tag: &str) -> Result<Vec<ServerInfo>> {
        let output = self
            .run_command(&["server", "list", "--tags", tag, "--output-type", "json"])
            .await?;

        if output.trim().is_empty() || output.trim() == "[]" {
            return Ok(Vec::new());
        }

        let servers: Vec<ServerInfo> = serde_json::from_str(&output)?;
        Ok(servers)
    }

    /// Find a single server by FleetFlow tag
    pub async fn find_server_by_fleetflow_tag(&self, project: &str, server_name: &str) -> Result<Option<ServerInfo>> {
        let tag = format!("fleetflow:{}:{}", project, server_name);
        let servers = self.find_servers_by_tag(&tag).await?;
        Ok(servers.into_iter().next())
    }

    /// Get server by name
    pub async fn get_server(&self, name: &str) -> Result<Option<ServerInfo>> {
        let servers = self.list_servers().await?;
        Ok(servers.into_iter().find(|s| s.name == name))
    }

    /// Get server by ID
    pub async fn get_server_by_id(&self, id: &str) -> Result<ServerInfo> {
        let output = self
            .run_command(&["server", "read", id, "--output-type", "json"])
            .await?;

        let server: ServerInfo = serde_json::from_str(&output)?;
        Ok(server)
    }

    /// Create a server
    pub async fn create_server(&self, config: &CreateServerConfig) -> Result<ServerInfo> {
        // Store string conversions to extend their lifetime
        let core_str = config.core.to_string();
        let memory_str = config.memory.to_string();
        let disk_size_str = config.disk_size.map(|d| d.to_string());
        // Join tags with comma for usacloud
        let tags_str = config.tags.join(",");

        let mut args = vec![
            "server",
            "create",
            "--name",
            config.name.as_str(),
            "--core",
            core_str.as_str(),
            "--memory",
            memory_str.as_str(),
            "--output-type",
            "json",
            "-y",
        ];

        // Add disk options
        if let Some(ref disk_size) = disk_size_str {
            args.push("--disk-size");
            args.push(disk_size.as_str());
        }

        // Add OS type (usacloud uses --disk-os-type)
        if let Some(ref os) = config.os_type {
            args.push("--disk-os-type");
            args.push(os.as_str());
        }

        // Add SSH key IDs (usacloud uses --disk-edit-ssh-key-ids)
        if let Some(ref ssh_key_ids) = config.ssh_key_ids {
            for id in ssh_key_ids {
                args.push("--disk-edit-ssh-key-ids");
                args.push(id.as_str());
            }
        }

        // Add tags
        if !config.tags.is_empty() {
            args.push("--tags");
            args.push(tags_str.as_str());
        }

        // Connect to shared network for public IP
        args.push("--network-interface-upstream");
        args.push("shared");

        let output = self.run_command(&args).await?;

        // usacloud server create returns an array of servers
        let servers: Vec<ServerInfo> = serde_json::from_str(&output)?;
        servers
            .into_iter()
            .next()
            .ok_or_else(|| SakuraError::CommandFailed("サーバー作成結果が空です".to_string()))
    }

    /// Delete a server
    pub async fn delete_server(&self, id: &str, with_disks: bool) -> Result<()> {
        let mut args = vec!["server", "delete", id, "-y"];

        if with_disks {
            args.push("--with-disks");
        }

        self.run_command(&args).await?;
        Ok(())
    }

    /// Power on a server
    pub async fn power_on(&self, id: &str) -> Result<()> {
        self.run_command(&["server", "power-on", id, "-y"]).await?;
        Ok(())
    }

    /// Power off a server (graceful shutdown)
    pub async fn power_off(&self, id: &str) -> Result<()> {
        self.run_command(&["server", "shutdown", id, "-y"]).await?;
        Ok(())
    }

    /// List SSH keys (global resource, no zone needed)
    pub async fn list_ssh_keys(&self) -> Result<Vec<SshKeyInfo>> {
        let output = self
            .run_command_global(&["ssh-key", "list", "--output-type", "json"])
            .await?;

        if output.trim().is_empty() || output.trim() == "[]" {
            return Ok(Vec::new());
        }

        let keys: Vec<SshKeyInfo> = serde_json::from_str(&output)?;
        Ok(keys)
    }

    /// Create SSH key (global resource, no zone needed)
    pub async fn create_ssh_key(&self, name: &str, public_key: &str) -> Result<SshKeyInfo> {
        let output = self
            .run_command_global(&[
                "ssh-key",
                "create",
                "--name",
                name,
                "--public-key",
                public_key,
                "--output-type",
                "json",
            ])
            .await?;

        let key: SshKeyInfo = serde_json::from_str(&output)?;
        Ok(key)
    }
}

/// Authentication status from usacloud
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsacloudAuth {
    #[serde(rename = "Account")]
    pub account: Option<AccountInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
}

/// Server information from usacloud
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    #[serde(rename = "ID")]
    pub id: u64,

    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "CPU")]
    pub cpu: Option<i32>,

    #[serde(rename = "MemoryMB")]
    pub memory_mb: Option<i32>,

    #[serde(rename = "InstanceStatus")]
    pub instance_status: Option<String>,

    #[serde(rename = "Interfaces")]
    pub interfaces: Option<Vec<InterfaceInfo>>,

    #[serde(rename = "Tags", default)]
    pub tags: Vec<String>,
}

impl ServerInfo {
    /// Get ID as string
    pub fn id_str(&self) -> String {
        self.id.to_string()
    }

    /// Get the first IP address
    pub fn ip_address(&self) -> Option<String> {
        self.interfaces
            .as_ref()?
            .iter()
            .find_map(|i| i.ip_address.clone())
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        self.instance_status.as_deref() == Some("up")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceInfo {
    #[serde(rename = "IPAddress")]
    pub ip_address: Option<String>,
}

/// Configuration for creating a server
#[derive(Debug, Clone)]
pub struct CreateServerConfig {
    pub name: String,
    pub core: i32,
    pub memory: i32,
    pub disk_size: Option<i32>,
    pub os_type: Option<String>,
    pub ssh_key_ids: Option<Vec<String>>,
    pub tags: Vec<String>,
}

impl CreateServerConfig {
    /// Create FleetFlow tags for this server
    pub fn fleetflow_tags(project: &str, server_name: &str) -> Vec<String> {
        vec![
            format!("fleetflow:{}:{}", project, server_name),
            format!("fleetflow:project:{}", project),
        ]
    }
}

/// SSH key information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKeyInfo {
    #[serde(rename = "ID")]
    pub id: u64,

    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "PublicKey")]
    pub public_key: Option<String>,
}

impl SshKeyInfo {
    /// Get ID as string
    pub fn id_str(&self) -> String {
        self.id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_info_ip() {
        let server = ServerInfo {
            id: "123".to_string(),
            name: "test".to_string(),
            cpu: Some(4),
            memory_mb: Some(4096),
            instance_status: Some("up".to_string()),
            interfaces: Some(vec![InterfaceInfo {
                ip_address: Some("192.168.1.1".to_string()),
            }]),
            tags: vec!["fleetflow:test:server".to_string()],
        };

        assert_eq!(server.ip_address(), Some("192.168.1.1".to_string()));
        assert!(server.is_running());
        assert!(server.tags.contains(&"fleetflow:test:server".to_string()));
    }
}
