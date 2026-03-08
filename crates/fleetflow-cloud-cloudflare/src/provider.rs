//! Cloudflare provider implementation
//!
//! DNS record management via Cloudflare API.
//! R2 bucket management via wrangler CLI.

use crate::dns::{CloudflareDns, DnsConfig};
use crate::error::CloudflareError;
use crate::wrangler::Wrangler;
use async_trait::async_trait;
use fleetflow_cloud::{
    Action, ActionType, ApplyResult, AuthStatus, CloudProvider, Plan, ProviderState, ResourceSet,
    ResourceState, ResourceStatus,
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

    /// DNS クライアントを環境変数から生成（必要時のみ）
    fn create_dns_client() -> Result<CloudflareDns, CloudflareError> {
        let config = DnsConfig::from_env()?;
        Ok(CloudflareDns::new(config))
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
        let mut state = ProviderState::new();

        // DNS レコード状態を取得
        if let Ok(dns) = Self::create_dns_client()
            && let Ok(records) = dns.list_records().await
        {
            for record in records {
                let subdomain = record
                    .name
                    .trim_end_matches(&format!(".{}", dns.domain()))
                    .to_string();
                let key = format!("dns-a-{}", subdomain);
                let resource = ResourceState::new(&record.id, "dns-record")
                    .with_status(ResourceStatus::Running)
                    .with_attribute("record_type", serde_json::json!("A"))
                    .with_attribute("content", serde_json::json!(record.content))
                    .with_attribute("name", serde_json::json!(record.name))
                    .with_attribute("record_id", serde_json::json!(&record.id));

                state.add(key, resource);
            }
        }

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

        // DNS レコードの plan
        for resource in desired.iter() {
            if resource.resource_type != "dns-record" {
                continue;
            }

            let hostname = resource
                .get_config::<String>("hostname")
                .unwrap_or_default();
            let record_type = resource
                .get_config::<String>("record_type")
                .unwrap_or_default();

            let current_resource = current.get(&resource.id);

            match current_resource {
                None => {
                    actions.push(Action {
                        id: format!("create-{}", resource.id),
                        action_type: ActionType::Create,
                        resource_type: "dns-record".to_string(),
                        resource_id: resource.id.clone(),
                        description: format!("{} レコード {} を作成", record_type, hostname),
                        details: [
                            ("record_type".to_string(), serde_json::json!(record_type)),
                            ("hostname".to_string(), serde_json::json!(hostname)),
                        ]
                        .into_iter()
                        .collect(),
                    });
                }
                Some(_existing) => {
                    actions.push(Action {
                        id: format!("noop-{}", resource.id),
                        action_type: ActionType::NoOp,
                        resource_type: "dns-record".to_string(),
                        resource_id: resource.id.clone(),
                        description: format!("{} レコード {} は既に存在", record_type, hostname),
                        details: Default::default(),
                    });
                }
            }
        }

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
                    "dns-record" => {
                        let hostname = action
                            .details
                            .get("hostname")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let record_type = action
                            .details
                            .get("record_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("A");

                        match Self::create_dns_client() {
                            Ok(dns) => {
                                let dns_result = match record_type {
                                    "CNAME" => {
                                        let target = action
                                            .details
                                            .get("target")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let target_fqdn = dns.full_domain(target);
                                        dns.ensure_cname_record(hostname, &target_fqdn).await
                                    }
                                    _ => {
                                        // A レコード: IP は details["ip"] から取得
                                        // IP が未指定の場合はスキップ（サーバー作成後に設定）
                                        let ip = action
                                            .details
                                            .get("ip")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        if ip.is_empty() {
                                            result.add_success(
                                                action.id.clone(),
                                                format!(
                                                    "DNS A レコード {} はサーバー IP 取得後に作成されます",
                                                    hostname
                                                ),
                                            );
                                            continue;
                                        }
                                        dns.ensure_record(hostname, ip).await
                                    }
                                };

                                match dns_result {
                                    Ok(_record) => {
                                        result.add_success(
                                            action.id.clone(),
                                            format!(
                                                "DNS {} レコード {} を作成しました",
                                                record_type,
                                                dns.full_domain(hostname)
                                            ),
                                        );
                                    }
                                    Err(e) => {
                                        result.add_failure(action.id.clone(), e.to_string());
                                    }
                                }
                            }
                            Err(e) => {
                                result.add_failure(
                                    action.id.clone(),
                                    format!("DNS クライアント初期化失敗: {}", e),
                                );
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
                    "dns-record" => {
                        let hostname = action
                            .details
                            .get("hostname")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let record_type = action
                            .details
                            .get("record_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("A");

                        match Self::create_dns_client() {
                            Ok(dns) => {
                                let dns_result = match record_type {
                                    "CNAME" => dns.remove_cname_record(hostname).await,
                                    _ => dns.remove_record(hostname).await,
                                };

                                match dns_result {
                                    Ok(()) => {
                                        result.add_success(
                                            action.id.clone(),
                                            format!(
                                                "DNS {} レコード {} を削除しました",
                                                record_type,
                                                dns.full_domain(hostname)
                                            ),
                                        );
                                    }
                                    Err(e) => {
                                        result.add_failure(action.id.clone(), e.to_string());
                                    }
                                }
                            }
                            Err(e) => {
                                result.add_failure(
                                    action.id.clone(),
                                    format!("DNS クライアント初期化失敗: {}", e),
                                );
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
        if resource_id.starts_with("dns-a-") || resource_id.starts_with("dns-cname-") {
            // DNS レコード削除
            let hostname = resource_id
                .trim_start_matches("dns-a-")
                .trim_start_matches("dns-cname-");
            let is_cname = resource_id.starts_with("dns-cname-");

            let dns = Self::create_dns_client()
                .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;

            if is_cname {
                dns.remove_cname_record(hostname)
                    .await
                    .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;
            } else {
                dns.remove_record(hostname)
                    .await
                    .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;
            }
        } else {
            // R2 バケット削除
            self.wrangler
                .delete_r2_bucket(resource_id)
                .await
                .map_err(|e| fleetflow_cloud::CloudError::ApiError(e.to_string()))?;
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        let provider = CloudflareProvider::new(Some("acc-123".to_string()));
        assert_eq!(provider.name(), "cloudflare");
        assert_eq!(provider.display_name(), "Cloudflare");
    }

    #[test]
    fn test_provider_new_without_account_id() {
        let provider = CloudflareProvider::new(None);
        assert_eq!(provider.name(), "cloudflare");
    }
}
