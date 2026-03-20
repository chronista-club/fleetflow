//! AWS ServerProvider 実装
//!
//! EC2 インスタンスの CRUD 操作を提供する。

use aws_sdk_ec2::Client as Ec2Client;
use fleetflow_cloud::CloudError;
use fleetflow_cloud::server_provider::{CreateServerRequest, ServerSpec, ServerStatus};
use tracing::{debug, info};

use crate::instance_type::resolve_instance_type;

/// AWS サーバープロバイダ
pub struct AwsServerProvider {
    client: Ec2Client,
    /// CloudProvider (Phase 4) で使用予定
    #[allow(dead_code)]
    region: String,
}

impl AwsServerProvider {
    /// AWS SDK の標準認証チェーンから初期化
    pub async fn new(region: &str) -> Result<Self, CloudError> {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_ec2::config::Region::new(region.to_string()))
            .load()
            .await;

        let client = Ec2Client::new(&config);

        Ok(Self {
            client,
            region: region.to_string(),
        })
    }

    /// 既存クライアントから構築（テスト用）
    pub fn from_client(client: Ec2Client, region: String) -> Self {
        Self { client, region }
    }

    /// OS 文字列から AMI ID を解決する
    ///
    /// SSM パラメータストアから最新の AMI を取得する代わりに、
    /// EC2 の describe_images で canonical/amazon の公式 AMI を検索する。
    async fn resolve_ami(&self, os_type: &str) -> Result<String, CloudError> {
        let (owner, name_pattern) = match os_type {
            s if s.contains("ubuntu-24.04") || s.contains("ubuntu-noble") => (
                "099720109477",
                "ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-amd64-server-*",
            ),
            s if s.contains("ubuntu-22.04") || s.contains("ubuntu-jammy") => (
                "099720109477",
                "ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*",
            ),
            s if s.contains("amazon-linux") || s.contains("al2023") => {
                ("137112412989", "al2023-ami-2023.*-x86_64")
            }
            s if s.contains("debian-12") || s.contains("debian-bookworm") => {
                ("136693071363", "debian-12-amd64-*")
            }
            _ => {
                return Err(CloudError::InvalidConfig(format!(
                    "Unsupported OS type: {os_type}. Supported: ubuntu-24.04, ubuntu-22.04, amazon-linux, debian-12"
                )));
            }
        };

        let resp = self
            .client
            .describe_images()
            .owners(owner)
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("name")
                    .values(name_pattern)
                    .build(),
            )
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("state")
                    .values("available")
                    .build(),
            )
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("describe_images failed: {e}")))?;

        // 最新の AMI を選択（creation_date 降順）
        let mut images: Vec<_> = resp.images().to_vec();
        images.sort_by(|a, b| {
            b.creation_date()
                .unwrap_or_default()
                .cmp(a.creation_date().unwrap_or_default())
        });

        images
            .first()
            .and_then(|img| img.image_id().map(String::from))
            .ok_or_else(|| CloudError::ApiError(format!("No AMI found for OS: {os_type}")))
    }

    /// EC2 Instance → ServerSpec 変換
    fn instance_to_spec(&self, instance: &aws_sdk_ec2::types::Instance) -> ServerSpec {
        let instance_id = instance.instance_id().unwrap_or_default().to_string();
        let name = instance
            .tags()
            .iter()
            .find(|t| t.key() == Some("Name"))
            .and_then(|t| t.value())
            .unwrap_or_default()
            .to_string();

        let status = match instance.state().and_then(|s| s.name()) {
            Some(aws_sdk_ec2::types::InstanceStateName::Running) => ServerStatus::Running,
            Some(aws_sdk_ec2::types::InstanceStateName::Stopped) => ServerStatus::Stopped,
            _ => ServerStatus::Unknown,
        };

        // vCPU / memory はインスタンスタイプから推定せず、タグに保存した値を使う
        let cpu = instance
            .tags()
            .iter()
            .find(|t| t.key() == Some("fleetflow.cpu"))
            .and_then(|t| t.value())
            .and_then(|v| v.parse().ok());

        let memory_gb = instance
            .tags()
            .iter()
            .find(|t| t.key() == Some("fleetflow.memory_gb"))
            .and_then(|t| t.value())
            .and_then(|v| v.parse().ok());

        ServerSpec {
            id: instance_id,
            name,
            cpu,
            memory_gb,
            disk_gb: None,
            status,
            ip_address: instance.public_ip_address().map(String::from),
            provider: "aws".into(),
            zone: instance
                .placement()
                .and_then(|p| p.availability_zone())
                .map(String::from),
            tags: instance
                .tags()
                .iter()
                .filter(|t| {
                    !t.key().unwrap_or_default().starts_with("fleetflow.")
                        && t.key() != Some("Name")
                })
                .map(|t| {
                    format!(
                        "{}={}",
                        t.key().unwrap_or_default(),
                        t.value().unwrap_or_default()
                    )
                })
                .collect(),
        }
    }
}

impl fleetflow_cloud::server_provider::ServerProvider for AwsServerProvider {
    fn provider_name(&self) -> &str {
        "aws"
    }

    async fn list_servers(&self) -> fleetflow_cloud::Result<Vec<ServerSpec>> {
        let resp = self
            .client
            .describe_instances()
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("tag:fleetflow.managed")
                    .values("true")
                    .build(),
            )
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("instance-state-name")
                    .values("running")
                    .values("stopped")
                    .build(),
            )
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("describe_instances failed: {e}")))?;

        let specs: Vec<ServerSpec> = resp
            .reservations()
            .iter()
            .flat_map(|r| r.instances())
            .map(|i| self.instance_to_spec(i))
            .collect();

        debug!("Listed {} AWS instances", specs.len());
        Ok(specs)
    }

    async fn get_server(&self, server_id: &str) -> fleetflow_cloud::Result<ServerSpec> {
        let resp = self
            .client
            .describe_instances()
            .instance_ids(server_id)
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("describe_instances failed: {e}")))?;

        resp.reservations()
            .iter()
            .flat_map(|r| r.instances())
            .next()
            .map(|i| self.instance_to_spec(i))
            .ok_or_else(|| CloudError::ResourceNotFound(format!("Instance {server_id} not found")))
    }

    async fn create_server(
        &self,
        request: &CreateServerRequest,
    ) -> fleetflow_cloud::Result<ServerSpec> {
        let instance_type_str = resolve_instance_type(request.cpu, request.memory_gb)
            .map_err(fleetflow_cloud::CloudError::from)?;

        let os_type = request.os_type.as_deref().unwrap_or("ubuntu-24.04");

        let ami_id = self.resolve_ami(os_type).await?;

        info!(
            name = %request.name,
            instance_type = %instance_type_str,
            ami = %ami_id,
            "Creating EC2 instance"
        );

        let instance_type = aws_sdk_ec2::types::InstanceType::from(instance_type_str);

        let mut run_req = self
            .client
            .run_instances()
            .image_id(&ami_id)
            .instance_type(instance_type)
            .min_count(1)
            .max_count(1)
            .tag_specifications(
                aws_sdk_ec2::types::TagSpecification::builder()
                    .resource_type(aws_sdk_ec2::types::ResourceType::Instance)
                    .tags(tag("Name", &request.name))
                    .tags(tag("fleetflow.managed", "true"))
                    .tags(tag("fleetflow.cpu", &request.cpu.to_string()))
                    .tags(tag("fleetflow.memory_gb", &request.memory_gb.to_string()))
                    .build(),
            );

        // Key Pair（指定がある場合のみ）
        if let Some(key) = request.ssh_keys.first() {
            run_req = run_req.key_name(key);
        }

        // ネットワーク設定
        if let Some(ref network) = request.network {
            if let Some(ref subnet_id) = network.subnet_id {
                run_req = run_req.subnet_id(subnet_id);
            }
            for sg_id in &network.security_group_ids {
                run_req = run_req.security_group_ids(sg_id);
            }
        }

        let resp = run_req
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("run_instances failed: {e}")))?;

        let instance = resp.instances().first().ok_or_else(|| {
            CloudError::ApiError("No instance returned from run_instances".into())
        })?;

        let instance_id = instance.instance_id().unwrap_or_default();
        info!(instance_id = %instance_id, "EC2 instance created");

        // run_instances 直後のステータスは pending。
        // IP アドレスも未割当の場合がある。
        // 起動完了の確認は get_server() で再取得すること。
        let status = match instance.state().and_then(|s| s.name()) {
            Some(aws_sdk_ec2::types::InstanceStateName::Running) => ServerStatus::Running,
            Some(aws_sdk_ec2::types::InstanceStateName::Stopped) => ServerStatus::Stopped,
            _ => ServerStatus::Unknown, // pending 等
        };

        Ok(ServerSpec {
            id: instance_id.to_string(),
            name: request.name.clone(),
            cpu: Some(request.cpu),
            memory_gb: Some(request.memory_gb),
            disk_gb: request.disk_gb,
            status,
            ip_address: instance.public_ip_address().map(String::from),
            provider: "aws".into(),
            zone: instance
                .placement()
                .and_then(|p| p.availability_zone())
                .map(String::from),
            tags: request.tags.clone(),
        })
    }

    async fn delete_server(
        &self,
        server_id: &str,
        _with_disks: bool,
    ) -> fleetflow_cloud::Result<()> {
        info!(instance_id = %server_id, "Terminating EC2 instance");

        self.client
            .terminate_instances()
            .instance_ids(server_id)
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("terminate_instances failed: {e}")))?;

        Ok(())
    }

    async fn power_on(&self, server_id: &str) -> fleetflow_cloud::Result<()> {
        info!(instance_id = %server_id, "Starting EC2 instance");

        self.client
            .start_instances()
            .instance_ids(server_id)
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("start_instances failed: {e}")))?;

        Ok(())
    }

    async fn power_off(&self, server_id: &str) -> fleetflow_cloud::Result<()> {
        info!(instance_id = %server_id, "Stopping EC2 instance");

        self.client
            .stop_instances()
            .instance_ids(server_id)
            .send()
            .await
            .map_err(|e| CloudError::ApiError(format!("stop_instances failed: {e}")))?;

        Ok(())
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
    fn test_tag_creation() {
        let t = tag("Name", "test-server");
        assert_eq!(t.key(), Some("Name"));
        assert_eq!(t.value(), Some("test-server"));
    }

    #[test]
    fn test_instance_type_resolution_in_create() {
        // create_server で使われるマッピングが正しいか
        assert_eq!(resolve_instance_type(2, 4).unwrap(), "t3.medium");
        assert_eq!(resolve_instance_type(4, 16).unwrap(), "t3.xlarge");
    }
}
