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
        let which = Command::new("which").arg("usacloud").output().await?;

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
    pub async fn find_server_by_fleetflow_tag(
        &self,
        project: &str,
        server_name: &str,
    ) -> Result<Option<ServerInfo>> {
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

        // Add archive or OS type (archive takes priority)
        if let Some(ref archive) = config.archive {
            args.push("--disk-source-archive-id");
            args.push(archive.as_str());
        } else if let Some(ref os) = config.os_type {
            // Add OS type (usacloud uses --disk-os-type)
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

        // Build note vars JSON array if provided
        // Format: [{"ID": 123456789012, "Variables": {"key": "value"}}]
        let note_vars_json: Option<String> = if let Some(ref note_vars) = config.note_vars {
            let notes_array: Vec<serde_json::Value> = note_vars
                .iter()
                .map(|(note_id, vars)| {
                    serde_json::json!({
                        "ID": note_id.parse::<u64>().unwrap_or(0),
                        "Variables": vars
                    })
                })
                .collect();
            Some(serde_json::to_string(&notes_array).unwrap_or_default())
        } else {
            None
        };

        // Add startup script notes with variables (if note_vars is provided)
        // Otherwise fall back to note_ids
        if let Some(ref json) = note_vars_json {
            args.push("--disk-edit-notes");
            args.push(json.as_str());
        } else if let Some(ref note_ids) = config.note_ids {
            for id in note_ids {
                args.push("--disk-edit-note-ids");
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

        // Boot the server after creation
        args.push("--boot-after-create");

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

    /// List notes (startup scripts) - global resource, no zone needed
    pub async fn list_notes(&self) -> Result<Vec<NoteInfo>> {
        let output = self
            .run_command_global(&["note", "list", "--output-type", "json"])
            .await?;

        if output.trim().is_empty() || output.trim() == "[]" {
            return Ok(Vec::new());
        }

        let notes: Vec<NoteInfo> = serde_json::from_str(&output)?;
        Ok(notes)
    }

    /// Find a note by name
    pub async fn find_note_by_name(&self, name: &str) -> Result<Option<NoteInfo>> {
        let notes = self.list_notes().await?;
        Ok(notes.into_iter().find(|n| n.name == name))
    }

    /// Create a note (startup script) - global resource, no zone needed
    pub async fn create_note(&self, name: &str, content: &str, class: &str) -> Result<NoteInfo> {
        let output = self
            .run_command_global(&[
                "note",
                "create",
                "--name",
                name,
                "--content",
                content,
                "--class",
                class,
                "--output-type",
                "json",
            ])
            .await?;

        let note: NoteInfo = serde_json::from_str(&output)?;
        Ok(note)
    }

    /// Get or create a note by name
    pub async fn get_or_create_note(
        &self,
        name: &str,
        content: &str,
        class: &str,
    ) -> Result<NoteInfo> {
        if let Some(note) = self.find_note_by_name(name).await? {
            return Ok(note);
        }
        self.create_note(name, content, class).await
    }

    /// List archives
    pub async fn list_archives(&self) -> Result<Vec<ArchiveInfo>> {
        let output = self
            .run_command(&["archive", "list", "--output-type", "json"])
            .await?;

        if output.trim().is_empty() || output.trim() == "[]" {
            return Ok(Vec::new());
        }

        let archives: Vec<ArchiveInfo> = serde_json::from_str(&output)?;
        Ok(archives)
    }

    /// Find archive by name
    pub async fn find_archive_by_name(&self, name: &str) -> Result<Option<ArchiveInfo>> {
        let archives = self.list_archives().await?;
        Ok(archives.into_iter().find(|a| a.name == name))
    }

    /// Resolve archive name or ID to ID
    /// If input is already a numeric ID, returns it as-is
    /// Otherwise, looks up by name
    pub async fn resolve_archive_id(&self, name_or_id: &str) -> Result<String> {
        // If it's already a numeric ID, return it
        if name_or_id.chars().all(|c| c.is_ascii_digit()) {
            return Ok(name_or_id.to_string());
        }

        // Otherwise, look up by name
        match self.find_archive_by_name(name_or_id).await? {
            Some(archive) => Ok(archive.id_str()),
            None => Err(SakuraError::CommandFailed(format!(
                "アーカイブが見つかりません: {}",
                name_or_id
            ))),
        }
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
    /// アーカイブIDまたは名前（os_type より優先）
    pub archive: Option<String>,
    pub ssh_key_ids: Option<Vec<String>>,
    pub note_ids: Option<Vec<String>>,
    /// Note variables to pass to startup scripts
    /// Format: HashMap<note_id, HashMap<var_name, var_value>>
    pub note_vars:
        Option<std::collections::HashMap<String, std::collections::HashMap<String, String>>>,
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

/// Note (startup script) information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteInfo {
    #[serde(rename = "ID")]
    pub id: u64,

    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Class")]
    pub class: Option<String>,

    #[serde(rename = "Scope")]
    pub scope: Option<String>,
}

impl NoteInfo {
    /// Get ID as string
    pub fn id_str(&self) -> String {
        self.id.to_string()
    }
}

/// Archive information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    #[serde(rename = "ID")]
    pub id: u64,

    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "SizeMB")]
    pub size_mb: Option<i64>,

    #[serde(rename = "Scope")]
    pub scope: Option<String>,
}

impl ArchiveInfo {
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
            id: 123,
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
