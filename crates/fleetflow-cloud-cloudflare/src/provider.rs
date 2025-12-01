//! Cloudflare provider implementation
//!
//! This is a skeleton implementation for future development.

use crate::error::CloudflareError;
use crate::wrangler::Wrangler;
use async_trait::async_trait;
use fleetflow_cloud::{
    Action, ActionType, ApplyResult, AuthStatus, CloudProvider, Plan, ProviderState, ResourceSet,
};

/// Cloudflare provider
pub struct CloudflareProvider {
    wrangler: Wrangler,
    #[allow(dead_code)]
    account_id: Option<String>,
}

impl CloudflareProvider {
    pub fn new(account_id: Option<String>) -> Self {
        Self {
            wrangler: Wrangler::new(account_id.clone()),
            account_id,
        }
    }
}

#[async_trait]
impl CloudProvider for CloudflareProvider {
    fn name(&self) -> &str {
        "cloudflare"
    }

    fn display_name(&self) -> &str {
        "Cloudflare"
    }

    async fn check_auth(&self) -> fleetflow_cloud::Result<AuthStatus> {
        match self.wrangler.check_auth().await {
            Ok(auth) => {
                if auth.authenticated {
                    let account_info = auth.account_id.unwrap_or_else(|| "Unknown".to_string());
                    Ok(AuthStatus::ok(account_info))
                } else {
                    Ok(AuthStatus::failed("wrangler が認証されていません"))
                }
            }
            Err(CloudflareError::WranglerNotFound) => {
                Ok(AuthStatus::failed("wrangler がインストールされていません"))
            }
            Err(e) => Ok(AuthStatus::failed(e.to_string())),
        }
    }

    async fn get_state(&self) -> fleetflow_cloud::Result<ProviderState> {
        let state = ProviderState::new();

        // TODO: Implement R2 bucket state retrieval
        // let buckets = self.wrangler.list_r2_buckets().await...

        // TODO: Implement Worker state retrieval
        // let workers = self.wrangler.list_workers().await...

        Ok(state)
    }

    async fn plan(&self, desired: &ResourceSet) -> fleetflow_cloud::Result<Plan> {
        let current = self.get_state().await?;
        let mut actions = Vec::new();

        // Check for R2 buckets to create
        for resource in desired.iter() {
            if resource.resource_type != "r2-bucket" {
                continue;
            }

            let current_resource = current.get(&resource.id);

            match current_resource {
                None => {
                    actions.push(Action {
                        id: format!("create-{}", resource.id),
                        action_type: ActionType::Create,
                        resource_type: "r2-bucket".to_string(),
                        resource_id: resource.id.clone(),
                        description: format!("R2バケット {} を作成", resource.id),
                        details: [("provider".to_string(), serde_json::json!("cloudflare"))]
                            .into_iter()
                            .collect(),
                    });
                }
                Some(_existing) => {
                    actions.push(Action {
                        id: format!("noop-{}", resource.id),
                        action_type: ActionType::NoOp,
                        resource_type: "r2-bucket".to_string(),
                        resource_id: resource.id.clone(),
                        description: format!("R2バケット {} は既に存在します", resource.id),
                        details: Default::default(),
                    });
                }
            }
        }

        // TODO: Add Worker planning
        // TODO: Add DNS planning

        Ok(Plan::new(actions))
    }

    async fn apply(&self, plan: &Plan) -> fleetflow_cloud::Result<ApplyResult> {
        let mut result = ApplyResult::new();
        let start = std::time::Instant::now();

        for action in &plan.actions {
            match action.action_type {
                ActionType::Create => match action.resource_type.as_str() {
                    "r2-bucket" => {
                        tracing::info!("Creating R2 bucket: {}", action.resource_id);
                        match self.wrangler.create_r2_bucket(&action.resource_id).await {
                            Ok(_bucket) => {
                                result.add_success(
                                    action.id.clone(),
                                    format!("R2バケット {} を作成しました", action.resource_id),
                                );
                            }
                            Err(e) => {
                                result.add_failure(action.id.clone(), e.to_string());
                            }
                        }
                    }
                    _ => {
                        result.add_failure(
                            action.id.clone(),
                            format!("未対応のリソースタイプ: {}", action.resource_type),
                        );
                    }
                },
                ActionType::Delete => match action.resource_type.as_str() {
                    "r2-bucket" => {
                        tracing::info!("Deleting R2 bucket: {}", action.resource_id);
                        match self.wrangler.delete_r2_bucket(&action.resource_id).await {
                            Ok(()) => {
                                result.add_success(
                                    action.id.clone(),
                                    format!("R2バケット {} を削除しました", action.resource_id),
                                );
                            }
                            Err(e) => {
                                result.add_failure(action.id.clone(), e.to_string());
                            }
                        }
                    }
                    _ => {
                        result.add_failure(
                            action.id.clone(),
                            format!("未対応のリソースタイプ: {}", action.resource_type),
                        );
                    }
                },
                ActionType::Update => {
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
        // Try to delete as R2 bucket first
        // TODO: Need to determine resource type from ID or state
        self.wrangler
            .delete_r2_bucket(resource_id)
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;

        Ok(())
    }

    async fn destroy_all(&self) -> fleetflow_cloud::Result<ApplyResult> {
        let mut result = ApplyResult::new();
        let start = std::time::Instant::now();

        // Delete all R2 buckets
        let buckets = self
            .wrangler
            .list_r2_buckets()
            .await
            .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;

        for bucket in buckets {
            match self.wrangler.delete_r2_bucket(&bucket.name).await {
                Ok(()) => {
                    result.add_success(
                        format!("delete-{}", bucket.name),
                        format!("R2バケット {} を削除しました", bucket.name),
                    );
                }
                Err(e) => {
                    result.add_failure(format!("delete-{}", bucket.name), e.to_string());
                }
            }
        }

        // TODO: Delete Workers
        // TODO: Delete DNS records

        result.duration_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }
}
