//! Sakura Cloud provider implementation

use crate::error::{Result, SakuraError};
use crate::usacloud::{CreateServerConfig, Usacloud};
use async_trait::async_trait;
use fleetflow_cloud::{
    Action, ActionType, ApplyResult, AuthStatus, CloudProvider, Plan, ProviderState,
    ResourceConfig, ResourceSet, ResourceState, ResourceStatus,
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
    pub ssh_keys: Vec<String>,
    pub startup_scripts: Vec<String>,
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

        // Look up startup script (note) IDs
        let note_ids = if options.startup_scripts.is_empty() {
            None
        } else {
            let all_notes = self.usacloud.list_notes().await?;
            let ids: Vec<String> = options
                .startup_scripts
                .iter()
                .filter_map(|name| {
                    all_notes
                        .iter()
                        .find(|n| n.name == *name)
                        .map(|n| n.id_str())
                })
                .collect();
            if ids.is_empty() { None } else { Some(ids) }
        };

        let config = CreateServerConfig {
            name: options.name.clone(),
            core,
            memory,
            disk_size: options.disk_size,
            os_type: options.os.clone(),
            ssh_key_ids,
            note_ids,
            tags: options.tags.clone(),
        };

        let server = self.usacloud.create_server(&config).await?;
        Ok(server.into())
    }

    /// Delete a server
    pub async fn delete_server(&self, id: &str, with_disks: bool) -> Result<()> {
        self.usacloud.delete_server(id, with_disks).await
    }

    /// Parse server configuration from ResourceConfig
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
                    actions.push(Action {
                        id: format!("create-{}", resource.id),
                        action_type: ActionType::Create,
                        resource_type: "server".to_string(),
                        resource_id: resource.id.clone(),
                        description: format!("サーバー {} を作成", resource.id),
                        details: [
                            ("provider".to_string(), serde_json::json!("sakura-cloud")),
                            ("zone".to_string(), serde_json::json!(self.zone)),
                        ]
                        .into_iter()
                        .collect(),
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

                    // TODO: Get proper config from action.details
                    let config = CreateServerConfig {
                        name: action.resource_id.clone(),
                        core: 1,
                        memory: 1,
                        disk_size: Some(20),
                        os_type: Some("ubuntu2404".to_string()),
                        ssh_key_ids: None,
                        note_ids: None,
                        tags: Vec::new(),
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
