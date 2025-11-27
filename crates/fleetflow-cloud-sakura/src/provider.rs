//! Sakura Cloud provider implementation

use crate::error::{Result, SakuraError};
use crate::usacloud::{CreateServerConfig, Usacloud};
use async_trait::async_trait;
use fleetflow_cloud::{
    Action, ActionType, ApplyResult, AuthStatus, CloudProvider, Plan, ProviderState,
    ResourceConfig, ResourceSet, ResourceState, ResourceStatus,
};

/// Sakura Cloud provider
pub struct SakuraCloudProvider {
    usacloud: Usacloud,
    zone: String,
}

impl SakuraCloudProvider {
    pub fn new(zone: impl Into<String>) -> Self {
        let zone = zone.into();
        Self {
            usacloud: Usacloud::new(&zone),
            zone,
        }
    }

    /// Parse server configuration from ResourceConfig
    fn parse_server_config(&self, config: &ResourceConfig) -> Result<ServerResourceConfig> {
        let core = config
            .get_config::<i32>("core")
            .unwrap_or(1);
        let memory = config
            .get_config::<i32>("memory")
            .unwrap_or(1);
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

            let mut resource = ResourceState::new(&server.id, "server").with_status(status);

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
                            match self.usacloud.delete_server(&server.id, true).await {
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
            .delete_server(&server.id, true)
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
            match self.usacloud.delete_server(&server.id, true).await {
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
