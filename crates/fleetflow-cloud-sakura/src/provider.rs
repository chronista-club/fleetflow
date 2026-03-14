//! Sakura Cloud provider implementation

use std::collections::HashMap;

use crate::error::{Result, SakuraError};
use crate::usacloud::{CreateServerConfig, Usacloud};
use async_trait::async_trait;
use fleetflow_cloud::server_provider::ServerProvider;
use fleetflow_cloud::{
    Action, ActionType, ApplyResult, AuthStatus, CloudProvider, CreateServerRequest, Plan,
    ProviderState, ResourceConfig, ResourceSet, ResourceState, ResourceStatus, ServerSpec,
    ServerStatus,
};

/// Parse plan string like "2core-4gb" to (core, memory_gb)
fn parse_plan(plan: &Option<String>) -> (i32, i32) {
    if let Some(p) = plan {
        // Try to parse "NcoreN-Mgb" format
        let parts: Vec<&str> = p.split('-').collect();
        if parts.len() == 2 {
            let core = parts[0]
                .trim_end_matches("core")
                .parse::<i32>()
                .unwrap_or(1);
            let memory = parts[1].trim_end_matches("gb").parse::<i32>().unwrap_or(1);
            return (core, memory);
        }
    }
    (1, 1) // Default: 1 core, 1GB
}

/// Sakura Cloud provider
pub struct SakuraCloudProvider {
    usacloud: Usacloud,
    zone: String,
}

/// Options for creating a server (simplified for CLI use)
#[derive(Debug, Clone)]
pub struct CreateServerOptions {
    pub name: String,
    pub plan: Option<String>,
    pub disk_size: Option<i32>,
    pub os: Option<String>,
    /// アーカイブ名またはID（os より優先）
    pub archive: Option<String>,
    pub ssh_keys: Vec<String>,
    pub startup_scripts: Vec<String>,
    /// Variables to pass to startup scripts
    /// Format: HashMap<var_name, var_value>
    pub init_script_vars: std::collections::HashMap<String, String>,
    pub tags: Vec<String>,
}

/// Simple server info returned to CLI
#[derive(Debug, Clone)]
pub struct SimpleServerInfo {
    pub id: String,
    pub name: String,
    pub is_running: bool,
    pub ip_address: Option<String>,
}

impl From<crate::usacloud::ServerInfo> for SimpleServerInfo {
    fn from(info: crate::usacloud::ServerInfo) -> Self {
        Self {
            id: info.id_str(),
            name: info.name.clone(),
            is_running: info.is_running(),
            ip_address: info.ip_address(),
        }
    }
}

impl SakuraCloudProvider {
    pub fn new(zone: impl Into<String>) -> Self {
        let zone = zone.into();
        Self {
            usacloud: Usacloud::new(&zone),
            zone,
        }
    }

    /// Find server by FleetFlow tag (for idempotent operations)
    pub async fn find_server_by_tag(
        &self,
        project: &str,
        server_name: &str,
    ) -> Result<Option<SimpleServerInfo>> {
        match self
            .usacloud
            .find_server_by_fleetflow_tag(project, server_name)
            .await
        {
            Ok(Some(server)) => Ok(Some(server.into())),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Create a new server with FleetFlow tags
    pub async fn create_server(&self, options: &CreateServerOptions) -> Result<SimpleServerInfo> {
        // Parse plan to get core and memory
        let (core, memory) = parse_plan(&options.plan);

        // Resolve archive name to ID (if provided)
        let archive_id = if let Some(ref archive) = options.archive {
            Some(self.usacloud.resolve_archive_id(archive).await?)
        } else {
            None
        };

        // Look up SSH key IDs
        let ssh_key_ids = if options.ssh_keys.is_empty() {
            None
        } else {
            let all_keys = self.usacloud.list_ssh_keys().await?;
            let ids: Vec<String> = options
                .ssh_keys
                .iter()
                .filter_map(|name| {
                    all_keys
                        .iter()
                        .find(|k| k.name == *name)
                        .map(|k| k.id_str())
                })
                .collect();
            if ids.is_empty() { None } else { Some(ids) }
        };

        // Look up startup script (note) IDs and build note_vars if provided
        // If init_script_vars is provided, use note_vars format instead of note_ids
        let has_init_script_vars = !options.init_script_vars.is_empty();
        let mut note_ids: Option<Vec<String>> = None;
        let mut note_vars: Option<
            std::collections::HashMap<String, std::collections::HashMap<String, String>>,
        > = None;

        if !options.startup_scripts.is_empty() {
            let mut ids: Vec<String> = Vec::new();
            let mut vars_map: std::collections::HashMap<
                String,
                std::collections::HashMap<String, String>,
            > = std::collections::HashMap::new();

            for name in &options.startup_scripts {
                // Check if it's a built-in script
                let note_id =
                    if let Some(content) = crate::startup_scripts::get_builtin_script(name) {
                        // Get or create the built-in script
                        let note = self
                            .usacloud
                            .get_or_create_note(name, content, "shell")
                            .await?;
                        Some(note.id_str())
                    } else {
                        // Look up existing script by name
                        self.usacloud
                            .find_note_by_name(name)
                            .await?
                            .map(|note| note.id_str())
                    };

                if let Some(id) = note_id {
                    if has_init_script_vars {
                        // Use note_vars format with variables
                        vars_map.insert(id.clone(), options.init_script_vars.clone());
                    } else {
                        ids.push(id);
                    }
                }
            }

            if has_init_script_vars && !vars_map.is_empty() {
                note_vars = Some(vars_map);
            } else if !ids.is_empty() {
                note_ids = Some(ids);
            }
        }

        let config = CreateServerConfig {
            name: options.name.clone(),
            core,
            memory,
            disk_size: options.disk_size,
            os_type: options.os.clone(),
            archive: archive_id,
            ssh_key_ids,
            note_ids,
            note_vars,
            tags: options.tags.clone(),
        };

        let server = self.usacloud.create_server(&config).await?;
        Ok(server.into())
    }

    /// Delete a server
    pub async fn delete_server(&self, id: &str, with_disks: bool) -> Result<()> {
        self.usacloud.delete_server(id, with_disks).await
    }

    /// サーバーを起動（電源ON）
    pub async fn power_on(&self, id: &str) -> Result<()> {
        self.usacloud.power_on(id).await
    }

    /// サーバーを停止（電源OFF、グレースフルシャットダウン）
    pub async fn power_off(&self, id: &str) -> Result<()> {
        self.usacloud.power_off(id).await
    }

    /// Parse server configuration from ResourceConfig
    // TODO: この関数は将来のサーバー管理機能で使用予定
    #[allow(dead_code)]
    fn parse_server_config(&self, config: &ResourceConfig) -> Result<ServerResourceConfig> {
        let core = config.get_config::<i32>("core").unwrap_or(1);
        let memory = config.get_config::<i32>("memory").unwrap_or(1);
        let disk_size = config.get_config::<i32>("disk_size");
        let os_type = config.get_config::<String>("os_type");
        let ssh_key = config.get_config::<String>("ssh_key");

        Ok(ServerResourceConfig {
            name: config.id.clone(),
            core,
            memory,
            disk_size,
            os_type,
            ssh_key,
        })
    }
}

// TODO: この構造体は将来のサーバー管理機能で使用予定
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ServerResourceConfig {
    name: String,
    core: i32,
    memory: i32,
    disk_size: Option<i32>,
    os_type: Option<String>,
    ssh_key: Option<String>,
}

/// usacloud ServerInfo → プロバイダー非依存 ServerSpec への変換
impl SakuraCloudProvider {
    fn to_server_spec(&self, info: &crate::usacloud::ServerInfo) -> ServerSpec {
        ServerSpec {
            id: info.id_str(),
            name: info.name.clone(),
            cpu: info.cpu,
            memory_gb: info.memory_mb.map(|mb| mb / 1024),
            disk_gb: None, // usacloud server list ではディスク情報が含まれない
            status: if info.is_running() {
                ServerStatus::Running
            } else {
                ServerStatus::Stopped
            },
            ip_address: info.ip_address(),
            provider: "sakura-cloud".into(),
            zone: Some(self.zone.clone()),
            tags: info.tags.clone(),
        }
    }
}

impl ServerProvider for SakuraCloudProvider {
    fn provider_name(&self) -> &str {
        "sakura-cloud"
    }

    async fn list_servers(&self) -> fleetflow_cloud::Result<Vec<ServerSpec>> {
        let servers = self
            .usacloud
            .list_servers()
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;

        Ok(servers.iter().map(|s| self.to_server_spec(s)).collect())
    }

    async fn get_server(&self, server_id: &str) -> fleetflow_cloud::Result<ServerSpec> {
        let server = self
            .usacloud
            .get_server_by_id(server_id)
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;

        Ok(self.to_server_spec(&server))
    }

    async fn create_server(
        &self,
        request: &CreateServerRequest,
    ) -> fleetflow_cloud::Result<ServerSpec> {
        let options = CreateServerOptions {
            name: request.name.clone(),
            plan: Some(format!("{}core-{}gb", request.cpu, request.memory_gb)),
            disk_size: request.disk_gb,
            os: request.os_type.clone(),
            archive: request
                .provider_config
                .as_ref()
                .and_then(|c| c.get("archive"))
                .and_then(|v| v.as_str())
                .map(String::from),
            ssh_keys: request.ssh_keys.clone(),
            startup_scripts: request
                .provider_config
                .as_ref()
                .and_then(|c| c.get("startup_scripts"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            init_script_vars: request
                .provider_config
                .as_ref()
                .and_then(|c| c.get("init_script_vars"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            tags: request.tags.clone(),
        };

        let simple = SakuraCloudProvider::create_server(self, &options)
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;

        // create_server returns SimpleServerInfo, convert to ServerSpec
        Ok(ServerSpec {
            id: simple.id,
            name: simple.name,
            cpu: Some(request.cpu),
            memory_gb: Some(request.memory_gb),
            disk_gb: request.disk_gb,
            status: if simple.is_running {
                ServerStatus::Running
            } else {
                ServerStatus::Stopped
            },
            ip_address: simple.ip_address,
            provider: "sakura-cloud".into(),
            zone: Some(self.zone.clone()),
            tags: request.tags.clone(),
        })
    }

    async fn delete_server(&self, server_id: &str, with_disks: bool) -> fleetflow_cloud::Result<()> {
        SakuraCloudProvider::delete_server(self, server_id, with_disks)
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))
    }

    async fn power_on(&self, server_id: &str) -> fleetflow_cloud::Result<()> {
        SakuraCloudProvider::power_on(self, server_id)
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))
    }

    async fn power_off(&self, server_id: &str) -> fleetflow_cloud::Result<()> {
        SakuraCloudProvider::power_off(self, server_id)
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))
    }
}

#[async_trait]
impl CloudProvider for SakuraCloudProvider {
    fn name(&self) -> &str {
        "sakura-cloud"
    }

    fn display_name(&self) -> &str {
        "さくらのクラウド"
    }

    async fn check_auth(&self) -> fleetflow_cloud::Result<AuthStatus> {
        match self.usacloud.check_auth().await {
            Ok(auth) => {
                let account_info = auth
                    .account
                    .map(|a| format!("{} ({})", a.name, a.id))
                    .unwrap_or_else(|| "Unknown".to_string());
                Ok(AuthStatus::ok(account_info))
            }
            Err(SakuraError::UsacloudNotFound) => {
                Ok(AuthStatus::failed("usacloud がインストールされていません"))
            }
            Err(e) => Ok(AuthStatus::failed(e.to_string())),
        }
    }

    async fn get_state(&self) -> fleetflow_cloud::Result<ProviderState> {
        let mut state = ProviderState::new();

        // Get all servers
        let servers = self
            .usacloud
            .list_servers()
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;

        for server in servers {
            let status = if server.is_running() {
                ResourceStatus::Running
            } else {
                ResourceStatus::Stopped
            };

            let mut resource = ResourceState::new(server.id_str(), "server").with_status(status);

            resource.set_attribute("name", serde_json::json!(server.name));

            if let Some(ip) = server.ip_address() {
                resource.set_attribute("ip", serde_json::json!(ip));
            }

            if let Some(cpu) = server.cpu {
                resource.set_attribute("cpu", serde_json::json!(cpu));
            }

            if let Some(memory) = server.memory_mb {
                resource.set_attribute("memory_mb", serde_json::json!(memory));
            }

            state.add(server.name.clone(), resource);
        }

        Ok(state)
    }

    async fn plan(&self, desired: &ResourceSet) -> fleetflow_cloud::Result<Plan> {
        let current = self.get_state().await?;
        let mut actions = Vec::new();

        // Check for resources to create or update
        for resource in desired.iter() {
            if resource.resource_type != "server" {
                continue;
            }

            let current_resource = current.get(&resource.id);

            match current_resource {
                None => {
                    // Resource doesn't exist, create it
                    let mut details: HashMap<String, serde_json::Value> = [
                        ("provider".to_string(), serde_json::json!("sakura-cloud")),
                        ("zone".to_string(), serde_json::json!(self.zone)),
                    ]
                    .into_iter()
                    .collect();
                    // Pass through resource config for apply()
                    if let serde_json::Value::Object(ref map) = resource.config {
                        for (k, v) in map {
                            details.insert(k.clone(), v.clone());
                        }
                    }
                    actions.push(Action {
                        id: format!("create-{}", resource.id),
                        action_type: ActionType::Create,
                        resource_type: "server".to_string(),
                        resource_id: resource.id.clone(),
                        description: format!("サーバー {} を作成", resource.id),
                        details,
                    });
                }
                Some(_existing) => {
                    // TODO: Check for configuration differences and add Update action if needed
                    // For now, we skip update checks
                    actions.push(Action {
                        id: format!("noop-{}", resource.id),
                        action_type: ActionType::NoOp,
                        resource_type: "server".to_string(),
                        resource_id: resource.id.clone(),
                        description: format!("サーバー {} は既に存在します", resource.id),
                        details: Default::default(),
                    });
                }
            }
        }

        // Check for resources to delete (not in desired state)
        for (id, _resource) in current.iter() {
            if desired.get("server", id).is_none() {
                // Resource exists but not in desired state
                // Note: We don't automatically delete unless explicitly requested
                tracing::debug!(
                    "Server {} exists but not in desired state (will not auto-delete)",
                    id
                );
            }
        }

        Ok(Plan::new(actions))
    }

    async fn apply(&self, plan: &Plan) -> fleetflow_cloud::Result<ApplyResult> {
        let mut result = ApplyResult::new();
        let start = std::time::Instant::now();

        for action in &plan.actions {
            match action.action_type {
                ActionType::Create => {
                    tracing::info!("Creating server: {}", action.resource_id);

                    // Extract config from action.details (populated by plan())
                    let plan_str = action
                        .details
                        .get("plan")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let (core, memory) = parse_plan(&plan_str);
                    let disk_size = action
                        .details
                        .get("disk_size")
                        .and_then(|v| v.as_i64())
                        .map(|v| v as i32);
                    let os_type = action
                        .details
                        .get("os")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let ssh_key_names: Vec<String> = action
                        .details
                        .get("ssh_keys")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let tags: Vec<String> = action
                        .details
                        .get("tags")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();

                    // Resolve SSH key names to IDs
                    let ssh_key_ids = if ssh_key_names.is_empty() {
                        None
                    } else {
                        match self.usacloud.list_ssh_keys().await {
                            Ok(all_keys) => {
                                let ids: Vec<String> = ssh_key_names
                                    .iter()
                                    .filter_map(|name| {
                                        all_keys
                                            .iter()
                                            .find(|k| k.name == *name)
                                            .map(|k| k.id_str())
                                    })
                                    .collect();
                                if ids.is_empty() { None } else { Some(ids) }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "SSH鍵一覧の取得に失敗");
                                None
                            }
                        }
                    };

                    // Resolve startup scripts
                    let startup_scripts: Vec<String> = action
                        .details
                        .get("startup_scripts")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let mut note_ids: Option<Vec<String>> = None;
                    if !startup_scripts.is_empty() {
                        let mut ids = Vec::new();
                        for name in &startup_scripts {
                            if let Some(content) = crate::startup_scripts::get_builtin_script(name)
                            {
                                if let Ok(note) = self
                                    .usacloud
                                    .get_or_create_note(name, content, "shell")
                                    .await
                                {
                                    ids.push(note.id_str());
                                }
                            } else if let Ok(Some(note)) =
                                self.usacloud.find_note_by_name(name).await
                            {
                                ids.push(note.id_str());
                            }
                        }
                        if !ids.is_empty() {
                            note_ids = Some(ids);
                        }
                    }

                    let config = CreateServerConfig {
                        name: action.resource_id.clone(),
                        core,
                        memory,
                        disk_size,
                        os_type,
                        archive: None,
                        ssh_key_ids,
                        note_ids,
                        note_vars: None,
                        tags,
                    };

                    match self.usacloud.create_server(&config).await {
                        Ok(server) => {
                            result.add_success(
                                action.id.clone(),
                                format!(
                                    "サーバー {} を作成しました (ID: {})",
                                    server.name, server.id
                                ),
                            );
                        }
                        Err(e) => {
                            result.add_failure(action.id.clone(), e.to_string());
                        }
                    }
                }
                ActionType::Delete => {
                    tracing::info!("Deleting server: {}", action.resource_id);

                    // Get server ID
                    match self.usacloud.get_server(&action.resource_id).await {
                        Ok(Some(server)) => {
                            match self.usacloud.delete_server(&server.id_str(), true).await {
                                Ok(()) => {
                                    result.add_success(
                                        action.id.clone(),
                                        format!("サーバー {} を削除しました", action.resource_id),
                                    );
                                }
                                Err(e) => {
                                    result.add_failure(action.id.clone(), e.to_string());
                                }
                            }
                        }
                        Ok(None) => {
                            result.add_failure(
                                action.id.clone(),
                                format!("サーバー {} が見つかりません", action.resource_id),
                            );
                        }
                        Err(e) => {
                            result.add_failure(action.id.clone(), e.to_string());
                        }
                    }
                }
                ActionType::Update => {
                    // TODO: Implement update logic
                    result.add_success(action.id.clone(), "更新は未実装です".to_string());
                }
                ActionType::NoOp => {
                    // Nothing to do
                }
            }
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }

    async fn destroy(&self, resource_id: &str) -> fleetflow_cloud::Result<()> {
        let server = self
            .usacloud
            .get_server(resource_id)
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?
            .ok_or_else(|| {
                fleetflow_cloud::CloudError::ResourceNotFound(resource_id.to_string())
            })?;

        self.usacloud
            .delete_server(&server.id_str(), true)
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;

        Ok(())
    }

    async fn destroy_all(&self) -> fleetflow_cloud::Result<ApplyResult> {
        let mut result = ApplyResult::new();
        let start = std::time::Instant::now();

        let servers = self
            .usacloud
            .list_servers()
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;

        for server in servers {
            match self.usacloud.delete_server(&server.id_str(), true).await {
                Ok(()) => {
                    result.add_success(
                        format!("delete-{}", server.name),
                        format!("サーバー {} を削除しました", server.name),
                    );
                }
                Err(e) => {
                    result.add_failure(format!("delete-{}", server.name), e.to_string());
                }
            }
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- parse_plan tests ----

    #[test]
    fn test_parse_plan_standard() {
        let (core, mem) = parse_plan(&Some("2core-4gb".to_string()));
        assert_eq!(core, 2);
        assert_eq!(mem, 4);
    }

    #[test]
    fn test_parse_plan_single_core() {
        let (core, mem) = parse_plan(&Some("1core-1gb".to_string()));
        assert_eq!(core, 1);
        assert_eq!(mem, 1);
    }

    #[test]
    fn test_parse_plan_large() {
        let (core, mem) = parse_plan(&Some("16core-64gb".to_string()));
        assert_eq!(core, 16);
        assert_eq!(mem, 64);
    }

    #[test]
    fn test_parse_plan_none() {
        let (core, mem) = parse_plan(&None);
        assert_eq!(core, 1);
        assert_eq!(mem, 1);
    }

    #[test]
    fn test_parse_plan_invalid_format_single_part() {
        // Only one part, no dash
        let (core, mem) = parse_plan(&Some("4core".to_string()));
        assert_eq!(core, 1);
        assert_eq!(mem, 1);
    }

    #[test]
    fn test_parse_plan_invalid_numbers() {
        // Non-numeric values default to 1
        let (core, mem) = parse_plan(&Some("xcore-ygb".to_string()));
        assert_eq!(core, 1);
        assert_eq!(mem, 1);
    }

    #[test]
    fn test_parse_plan_empty_string() {
        let (core, mem) = parse_plan(&Some("".to_string()));
        assert_eq!(core, 1);
        assert_eq!(mem, 1);
    }

    // ---- SimpleServerInfo tests ----

    #[test]
    fn test_simple_server_info_from_server_info() {
        let server_info = crate::usacloud::ServerInfo {
            id: 12345,
            name: "web-01".to_string(),
            cpu: Some(4),
            memory_mb: Some(4096),
            instance_status: Some("up".to_string()),
            interfaces: Some(vec![crate::usacloud::InterfaceInfo {
                ip_address: Some("203.0.113.10".to_string()),
            }]),
            tags: vec![],
        };

        let simple: SimpleServerInfo = server_info.into();
        assert_eq!(simple.id, "12345");
        assert_eq!(simple.name, "web-01");
        assert!(simple.is_running);
        assert_eq!(simple.ip_address, Some("203.0.113.10".to_string()));
    }

    #[test]
    fn test_simple_server_info_from_stopped_server() {
        let server_info = crate::usacloud::ServerInfo {
            id: 99,
            name: "stopped-srv".to_string(),
            cpu: None,
            memory_mb: None,
            instance_status: Some("down".to_string()),
            interfaces: None,
            tags: vec![],
        };

        let simple: SimpleServerInfo = server_info.into();
        assert_eq!(simple.id, "99");
        assert!(!simple.is_running);
        assert!(simple.ip_address.is_none());
    }

    // ---- SakuraCloudProvider basic tests ----

    #[test]
    fn test_provider_name() {
        let provider = SakuraCloudProvider::new("tk1a");
        assert_eq!(provider.name(), "sakura-cloud");
        assert_eq!(provider.display_name(), "さくらのクラウド");
    }

    #[test]
    fn test_parse_server_config() {
        let provider = SakuraCloudProvider::new("tk1a");
        let config = ResourceConfig::new(
            "server",
            "web-01",
            "sakura-cloud",
            serde_json::json!({
                "core": 4,
                "memory": 8,
                "disk_size": 100,
                "os_type": "ubuntu2404",
                "ssh_key": "my-key"
            }),
        );

        let parsed = provider.parse_server_config(&config).unwrap();
        assert_eq!(parsed.name, "web-01");
        assert_eq!(parsed.core, 4);
        assert_eq!(parsed.memory, 8);
        assert_eq!(parsed.disk_size, Some(100));
        assert_eq!(parsed.os_type, Some("ubuntu2404".to_string()));
        assert_eq!(parsed.ssh_key, Some("my-key".to_string()));
    }

    #[test]
    fn test_parse_server_config_defaults() {
        let provider = SakuraCloudProvider::new("tk1a");
        let config =
            ResourceConfig::new("server", "minimal", "sakura-cloud", serde_json::json!({}));

        let parsed = provider.parse_server_config(&config).unwrap();
        assert_eq!(parsed.core, 1);
        assert_eq!(parsed.memory, 1);
        assert!(parsed.disk_size.is_none());
        assert!(parsed.os_type.is_none());
        assert!(parsed.ssh_key.is_none());
    }
}
