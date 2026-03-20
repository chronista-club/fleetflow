//! AWS CloudProvider 実装
//!
//! Subnet / Security Group / Elastic IP の宣言型 plan/apply を提供する。
//! VPC は既存 ID 指定で、FleetFlow は Subnet / SG / EIP を作成・管理する。

use std::collections::HashMap;

use async_trait::async_trait;
use aws_sdk_ec2::Client as Ec2Client;
use fleetflow_cloud::action::{Action, ActionType, ApplyResult, Plan};
use fleetflow_cloud::provider::{AuthStatus, CloudProvider, ResourceSet};
use fleetflow_cloud::state::ProviderState;
use fleetflow_cloud::CloudError;
use tracing::{debug, info, warn};

use crate::models::{PortSpec, SecurityGroupConfig, SecurityGroupRule, SubnetConfig};

/// AWS CloudProvider（Subnet / SG / EIP の宣言型管理）
pub struct AwsCloudProvider {
    client: Ec2Client,
    region: String,
    vpc_id: String,
}

impl AwsCloudProvider {
    /// AWS SDK の標準認証チェーンから初期化
    pub async fn new(region: &str, vpc_id: &str) -> Result<Self, CloudError> {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_ec2::config::Region::new(region.to_string()))
            .load()
            .await;

        let client = Ec2Client::new(&config);

        Ok(Self {
            client,
            region: region.to_string(),
            vpc_id: vpc_id.to_string(),
        })
    }

    /// 既存クライアントから構築（テスト用）
    pub fn from_client(client: Ec2Client, region: String, vpc_id: String) -> Self {
        Self {
            client,
            region,
            vpc_id,
        }
    }

    // ── Subnet 操作 ──

    async fn create_subnet(&self, config: &SubnetConfig) -> Result<String, CloudError> {
        info!(name = %config.name, cidr = %config.cidr, az = %config.az, "Creating subnet");

        let resp = self
            .client
            .create_subnet()
            .vpc_id(&self.vpc_id)
            .cidr_block(&config.cidr)
            .availability_zone(&config.az)
            .tag_specifications(
                aws_sdk_ec2::types::TagSpecification::builder()
                    .resource_type(aws_sdk_ec2::types::ResourceType::Subnet)
                    .tags(tag("Name", &config.name))
                    .tags(tag("fleetflow.managed", "true"))
                    .build(),
            )
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("create_subnet failed: {e}")))?;

        let subnet_id = resp
            .subnet()
            .and_then(|s| s.subnet_id())
            .ok_or_else(|| CloudError::ApiError("No subnet ID returned".into()))?
            .to_string();

        info!(subnet_id = %subnet_id, "Subnet created");
        Ok(subnet_id)
    }

    async fn delete_subnet(&self, subnet_id: &str) -> Result<(), CloudError> {
        info!(subnet_id = %subnet_id, "Deleting subnet");

        self.client
            .delete_subnet()
            .subnet_id(subnet_id)
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("delete_subnet failed: {e}")))?;

        Ok(())
    }

    async fn list_managed_subnets(&self) -> Result<Vec<(String, String)>, CloudError> {
        let resp = self
            .client
            .describe_subnets()
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("vpc-id")
                    .values(&self.vpc_id)
                    .build(),
            )
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("tag:fleetflow.managed")
                    .values("true")
                    .build(),
            )
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("describe_subnets failed: {e}")))?;

        Ok(resp
            .subnets()
            .iter()
            .filter_map(|s| {
                let id = s.subnet_id()?.to_string();
                let name = s
                    .tags()
                    .iter()
                    .find(|t| t.key() == Some("Name"))
                    .and_then(|t| t.value())
                    .unwrap_or_default()
                    .to_string();
                Some((name, id))
            })
            .collect())
    }

    // ── Security Group 操作 ──

    async fn create_security_group(
        &self,
        config: &SecurityGroupConfig,
    ) -> Result<String, CloudError> {
        info!(name = %config.name, rules = config.inbound_rules.len(), "Creating security group");

        let resp = self
            .client
            .create_security_group()
            .group_name(format!("fleetflow-{}", config.name))
            .description(format!("FleetFlow managed SG: {}", config.name))
            .vpc_id(&self.vpc_id)
            .tag_specifications(
                aws_sdk_ec2::types::TagSpecification::builder()
                    .resource_type(aws_sdk_ec2::types::ResourceType::SecurityGroup)
                    .tags(tag("Name", &config.name))
                    .tags(tag("fleetflow.managed", "true"))
                    .build(),
            )
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("create_security_group failed: {e}")))?;

        let sg_id = resp
            .group_id()
            .ok_or_else(|| CloudError::ApiError("No SG ID returned".into()))?
            .to_string();

        // インバウンドルール追加
        if !config.inbound_rules.is_empty() {
            self.authorize_ingress(&sg_id, &config.inbound_rules)
                .await?;
        }

        info!(sg_id = %sg_id, "Security group created");
        Ok(sg_id)
    }

    async fn authorize_ingress(
        &self,
        sg_id: &str,
        rules: &[SecurityGroupRule],
    ) -> Result<(), CloudError> {
        let mut req = self
            .client
            .authorize_security_group_ingress()
            .group_id(sg_id);

        for rule in rules {
            let mut perm = aws_sdk_ec2::types::IpPermission::builder()
                .ip_protocol(&rule.protocol);

            match &rule.port {
                PortSpec::Single(port) => {
                    perm = perm.from_port(*port as i32).to_port(*port as i32);
                }
                PortSpec::Range(from, to) => {
                    perm = perm.from_port(*from as i32).to_port(*to as i32);
                }
                PortSpec::All => {
                    perm = perm.from_port(-1).to_port(-1);
                }
            }

            perm = perm.ip_ranges(
                aws_sdk_ec2::types::IpRange::builder()
                    .cidr_ip(&rule.from)
                    .build(),
            );

            req = req.ip_permissions(perm.build());
        }

        req.send()
            .await
            .map_err(|e| CloudError::ApiError(format!("authorize_ingress failed: {e}")))?;

        Ok(())
    }

    async fn delete_security_group(&self, sg_id: &str) -> Result<(), CloudError> {
        info!(sg_id = %sg_id, "Deleting security group");

        self.client
            .delete_security_group()
            .group_id(sg_id)
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("delete_security_group failed: {e}")))?;

        Ok(())
    }

    async fn list_managed_security_groups(&self) -> Result<Vec<(String, String)>, CloudError> {
        let resp = self
            .client
            .describe_security_groups()
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("vpc-id")
                    .values(&self.vpc_id)
                    .build(),
            )
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("tag:fleetflow.managed")
                    .values("true")
                    .build(),
            )
            .send()
            .await
            .map_err(|e| {
                CloudError::ApiError(format!("describe_security_groups failed: {e}"))
            })?;

        Ok(resp
            .security_groups()
            .iter()
            .filter_map(|sg| {
                let id = sg.group_id()?.to_string();
                let name = sg
                    .tags()
                    .iter()
                    .find(|t| t.key() == Some("Name"))
                    .and_then(|t| t.value())
                    .unwrap_or_default()
                    .to_string();
                Some((name, id))
            })
            .collect())
    }

    // ── Elastic IP 操作 ──

    async fn allocate_eip(&self, name: &str) -> Result<String, CloudError> {
        info!(name = %name, "Allocating Elastic IP");

        let resp = self
            .client
            .allocate_address()
            .domain(aws_sdk_ec2::types::DomainType::Vpc)
            .tag_specifications(
                aws_sdk_ec2::types::TagSpecification::builder()
                    .resource_type(aws_sdk_ec2::types::ResourceType::ElasticIp)
                    .tags(tag("Name", name))
                    .tags(tag("fleetflow.managed", "true"))
                    .build(),
            )
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("allocate_address failed: {e}")))?;

        let alloc_id = resp
            .allocation_id()
            .ok_or_else(|| CloudError::ApiError("No allocation ID returned".into()))?
            .to_string();

        info!(alloc_id = %alloc_id, "Elastic IP allocated");
        Ok(alloc_id)
    }

    async fn release_eip(&self, alloc_id: &str) -> Result<(), CloudError> {
        info!(alloc_id = %alloc_id, "Releasing Elastic IP");

        self.client
            .release_address()
            .allocation_id(alloc_id)
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("release_address failed: {e}")))?;

        Ok(())
    }
}

#[async_trait]
impl CloudProvider for AwsCloudProvider {
    fn name(&self) -> &str {
        "aws"
    }

    fn display_name(&self) -> &str {
        "Amazon Web Services"
    }

    async fn check_auth(&self) -> fleetflow_cloud::Result<AuthStatus> {
        match self
            .client
            .describe_regions()
            .region_names(&self.region)
            .send()
            .await
        {
            Ok(_resp) => {
                Ok(AuthStatus::ok(format!(
                    "AWS authenticated (region: {}, vpc: {})",
                    self.region, self.vpc_id
                )))
            }
            Err(e) => Ok(AuthStatus::failed(format!("AWS auth failed: {e}"))),
        }
    }

    async fn get_state(&self) -> fleetflow_cloud::Result<ProviderState> {
        let subnets = self.list_managed_subnets().await?;
        let sgs = self.list_managed_security_groups().await?;

        debug!(
            subnets = subnets.len(),
            security_groups = sgs.len(),
            "AWS provider state retrieved"
        );

        Ok(ProviderState {
            resources: HashMap::new(), // 将来的に ResourceState を構築
        })
    }

    async fn plan(&self, desired: &ResourceSet) -> fleetflow_cloud::Result<Plan> {
        let existing_subnets = self.list_managed_subnets().await?;
        let existing_sgs = self.list_managed_security_groups().await?;

        let existing_subnet_names: Vec<&str> =
            existing_subnets.iter().map(|(n, _)| n.as_str()).collect();
        let existing_sg_names: Vec<&str> =
            existing_sgs.iter().map(|(n, _)| n.as_str()).collect();

        let mut actions = Vec::new();

        // Subnet の plan
        for resource in desired.by_type("subnet") {
            let name = &resource.id;
            if existing_subnet_names.contains(&name.as_str()) {
                actions.push(Action {
                    id: format!("subnet:{name}"),
                    action_type: ActionType::NoOp,
                    resource_type: "subnet".into(),
                    resource_id: name.clone(),
                    description: format!("Subnet '{name}' already exists"),
                    details: HashMap::new(),
                });
            } else {
                actions.push(Action {
                    id: format!("subnet:{name}"),
                    action_type: ActionType::Create,
                    resource_type: "subnet".into(),
                    resource_id: name.clone(),
                    description: format!("Create subnet '{name}'"),
                    details: [("config".to_string(), resource.config.clone())]
                        .into_iter()
                        .collect(),
                });
            }
        }

        // Security Group の plan
        for resource in desired.by_type("security-group") {
            let name = &resource.id;
            if existing_sg_names.contains(&name.as_str()) {
                actions.push(Action {
                    id: format!("sg:{name}"),
                    action_type: ActionType::NoOp,
                    resource_type: "security-group".into(),
                    resource_id: name.clone(),
                    description: format!("Security group '{name}' already exists"),
                    details: HashMap::new(),
                });
            } else {
                actions.push(Action {
                    id: format!("sg:{name}"),
                    action_type: ActionType::Create,
                    resource_type: "security-group".into(),
                    resource_id: name.clone(),
                    description: format!("Create security group '{name}'"),
                    details: [("config".to_string(), resource.config.clone())]
                        .into_iter()
                        .collect(),
                });
            }
        }

        // Elastic IP の plan
        for resource in desired.by_type("elastic-ip") {
            let name = &resource.id;
            actions.push(Action {
                id: format!("eip:{name}"),
                action_type: ActionType::Create,
                resource_type: "elastic-ip".into(),
                resource_id: name.clone(),
                description: format!("Allocate Elastic IP for '{name}'"),
                details: HashMap::new(),
            });
        }

        let plan = Plan::new(actions);
        let summary = plan.summary();
        info!(%summary, "AWS plan generated");
        Ok(plan)
    }

    async fn apply(&self, plan: &Plan) -> fleetflow_cloud::Result<ApplyResult> {
        let start = std::time::Instant::now();
        let mut result = ApplyResult::new();

        // 依存順: Subnet → SG → EIP
        for action in plan
            .actions_by_type(ActionType::Create)
            .iter()
            .filter(|a| a.resource_type == "subnet")
        {
            match self.apply_create_subnet(action).await {
                Ok(msg) => result.add_success(action.id.clone(), msg),
                Err(e) => result.add_failure(action.id.clone(), e.to_string()),
            }
        }

        for action in plan
            .actions_by_type(ActionType::Create)
            .iter()
            .filter(|a| a.resource_type == "security-group")
        {
            match self.apply_create_sg(action).await {
                Ok(msg) => result.add_success(action.id.clone(), msg),
                Err(e) => result.add_failure(action.id.clone(), e.to_string()),
            }
        }

        for action in plan
            .actions_by_type(ActionType::Create)
            .iter()
            .filter(|a| a.resource_type == "elastic-ip")
        {
            match self.allocate_eip(&action.resource_id).await {
                Ok(alloc_id) => {
                    result.add_success(action.id.clone(), format!("EIP allocated: {alloc_id}"))
                }
                Err(e) => result.add_failure(action.id.clone(), e.to_string()),
            }
        }

        // Delete は逆順: EIP → SG → Subnet
        for action in plan
            .actions_by_type(ActionType::Delete)
            .iter()
            .filter(|a| a.resource_type == "elastic-ip")
        {
            if let Some(alloc_id) = action.details.get("aws_id").and_then(|v| v.as_str()) {
                match self.release_eip(alloc_id).await {
                    Ok(()) => result.add_success(action.id.clone(), "EIP released".into()),
                    Err(e) => result.add_failure(action.id.clone(), e.to_string()),
                }
            }
        }

        for action in plan
            .actions_by_type(ActionType::Delete)
            .iter()
            .filter(|a| a.resource_type == "security-group")
        {
            if let Some(sg_id) = action.details.get("aws_id").and_then(|v| v.as_str()) {
                match self.delete_security_group(sg_id).await {
                    Ok(()) => result.add_success(action.id.clone(), "SG deleted".into()),
                    Err(e) => result.add_failure(action.id.clone(), e.to_string()),
                }
            }
        }

        for action in plan
            .actions_by_type(ActionType::Delete)
            .iter()
            .filter(|a| a.resource_type == "subnet")
        {
            if let Some(subnet_id) = action.details.get("aws_id").and_then(|v| v.as_str()) {
                match self.delete_subnet(subnet_id).await {
                    Ok(()) => result.add_success(action.id.clone(), "Subnet deleted".into()),
                    Err(e) => result.add_failure(action.id.clone(), e.to_string()),
                }
            }
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }

    async fn destroy(&self, resource_id: &str) -> fleetflow_cloud::Result<()> {
        warn!(resource_id = %resource_id, "Destroying AWS resource");
        // resource_id のフォーマット: "subnet:xxx" or "sg:xxx" or "eip:xxx"
        let parts: Vec<&str> = resource_id.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig(format!(
                "Invalid resource ID format: {resource_id}"
            )));
        }

        match parts[0] {
            "subnet" => self.delete_subnet(parts[1]).await,
            "sg" => self.delete_security_group(parts[1]).await,
            "eip" => self.release_eip(parts[1]).await,
            other => Err(CloudError::InvalidConfig(format!(
                "Unknown resource type: {other}"
            ))),
        }
    }

    async fn destroy_all(&self) -> fleetflow_cloud::Result<ApplyResult> {
        let start = std::time::Instant::now();
        let mut result = ApplyResult::new();

        // 逆順: SG → Subnet（EIP は list が未実装のため TODO）
        let sgs = self.list_managed_security_groups().await?;
        for (name, sg_id) in &sgs {
            match self.delete_security_group(sg_id).await {
                Ok(()) => result.add_success(format!("sg:{name}"), "SG deleted".into()),
                Err(e) => result.add_failure(format!("sg:{name}"), e.to_string()),
            }
        }

        let subnets = self.list_managed_subnets().await?;
        for (name, subnet_id) in &subnets {
            match self.delete_subnet(subnet_id).await {
                Ok(()) => result.add_success(format!("subnet:{name}"), "Subnet deleted".into()),
                Err(e) => result.add_failure(format!("subnet:{name}"), e.to_string()),
            }
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }
}

impl AwsCloudProvider {
    async fn apply_create_subnet(&self, action: &Action) -> Result<String, CloudError> {
        let config: SubnetConfig = action
            .details
            .get("config")
            .ok_or_else(|| CloudError::InvalidConfig("Missing subnet config".into()))
            .and_then(|v| {
                serde_json::from_value(v.clone())
                    .map_err(|e| CloudError::InvalidConfig(format!("Invalid subnet config: {e}")))
            })?;

        let subnet_id = self.create_subnet(&config).await?;
        Ok(format!("Subnet created: {subnet_id}"))
    }

    async fn apply_create_sg(&self, action: &Action) -> Result<String, CloudError> {
        let config: SecurityGroupConfig = action
            .details
            .get("config")
            .ok_or_else(|| CloudError::InvalidConfig("Missing SG config".into()))
            .and_then(|v| {
                serde_json::from_value(v.clone())
                    .map_err(|e| CloudError::InvalidConfig(format!("Invalid SG config: {e}")))
            })?;

        let sg_id = self.create_security_group(&config).await?;
        Ok(format!("Security group created: {sg_id}"))
    }
}

/// EC2 タグを簡易作成
fn tag(key: &str, value: &str) -> aws_sdk_ec2::types::Tag {
    aws_sdk_ec2::types::Tag::builder()
        .key(key)
        .value(value)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_with_new_resources() {
        let mut desired = ResourceSet::new();
        desired.add(ResourceConfig::new(
            "subnet",
            "web",
            "aws",
            serde_json::json!({
                "name": "web",
                "cidr": "10.0.1.0/24",
                "az": "ap-northeast-1a"
            }),
        ));
        desired.add(ResourceConfig::new(
            "security-group",
            "web-sg",
            "aws",
            serde_json::json!({
                "name": "web-sg",
                "inbound_rules": [{
                    "protocol": "tcp",
                    "port": {"Single": 80},
                    "from": "0.0.0.0/0"
                }]
            }),
        ));

        // ResourceSet の構築確認
        assert_eq!(desired.by_type("subnet").len(), 1);
        assert_eq!(desired.by_type("security-group").len(), 1);
    }

    #[test]
    fn test_resource_config_for_subnet() {
        let config = ResourceConfig::new(
            "subnet",
            "web",
            "aws",
            serde_json::json!({
                "name": "web",
                "cidr": "10.0.1.0/24",
                "az": "ap-northeast-1a"
            }),
        );

        let subnet: SubnetConfig = serde_json::from_value(config.config).unwrap();
        assert_eq!(subnet.name, "web");
        assert_eq!(subnet.cidr, "10.0.1.0/24");
    }

    #[test]
    fn test_resource_config_for_security_group() {
        let config = ResourceConfig::new(
            "security-group",
            "web-sg",
            "aws",
            serde_json::json!({
                "name": "web-sg",
                "inbound_rules": [{
                    "protocol": "tcp",
                    "port": {"Single": 80},
                    "from": "0.0.0.0/0"
                }]
            }),
        );

        let sg: SecurityGroupConfig = serde_json::from_value(config.config).unwrap();
        assert_eq!(sg.name, "web-sg");
        assert_eq!(sg.inbound_rules.len(), 1);
        assert_eq!(sg.inbound_rules[0].protocol, "tcp");
    }
}
