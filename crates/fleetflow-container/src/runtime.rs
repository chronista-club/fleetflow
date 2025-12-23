use anyhow::Result;
use bollard::Docker;
use fleetflow_core::Flow;
use std::path::PathBuf;
use tracing::{info, warn};

pub struct Runtime {
    pub docker: Docker,
    pub project_root: PathBuf,
}

impl Runtime {
    pub fn new(project_root: PathBuf) -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self {
            docker,
            project_root,
        })
    }

    /// 指定されたステージを起動する
    pub async fn up(&self, flow: &Flow, stage_name: &str, pull: bool) -> Result<()> {
        let stage = flow
            .stages
            .get(stage_name)
            .ok_or_else(|| anyhow::anyhow!("Stage '{}' not found", stage_name))?;

        info!("Starting stage: {}", stage_name);

        // 1. ネットワークの作成
        let network_name = crate::get_network_name(&flow.name, stage_name);
        self.ensure_network(&network_name).await?;

        // 2. 各サービスの起動（依存関係の考慮は将来の課題、現在は順次）
        for service_name in &stage.services {
            let service = flow
                .services
                .get(service_name)
                .ok_or_else(|| anyhow::anyhow!("Service '{}' not found", service_name))?;

            self.up_service(service_name, service, stage_name, &flow.name, pull)
                .await?;
        }

        Ok(())
    }

    async fn ensure_network(&self, name: &str) -> Result<()> {
        let network_config = bollard::models::NetworkCreateRequest {
            name: name.to_string(),
            driver: Some("bridge".to_string()),
            ..Default::default()
        };

        match self.docker.create_network(network_config).await {
            Ok(_) => info!("Network created: {}", name),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 409, ..
            }) => {
                debug!("Network already exists: {}", name);
            }
            Err(e) => warn!("Failed to create network {}: {}", name, e),
        }
        Ok(())
    }

    async fn up_service(
        &self,
        name: &str,
        service: &fleetflow_core::model::Service,
        stage_name: &str,
        project_name: &str,
        _pull: bool, // TODO: Pull ロジックの実装
    ) -> Result<()> {
        info!("Starting service: {}", name);

        // ホストポートの空きを確保
        for port in &service.ports {
            crate::port::ensure_port_available(port.host).await?;
        }

        let container_name = format!("{}-{}-{}", project_name, stage_name, name);
        let (container_config, create_options) =
            crate::service_to_container_config(name, service, stage_name, project_name);

        // コンテナの作成と起動
        match self
            .docker
            .create_container(Some(create_options), container_config)
            .await
        {
            Ok(_) => {
                self.docker
                    .start_container(
                        &container_name,
                        None::<bollard::query_parameters::StartContainerOptions>,
                    )
                    .await?;
                info!("Service {} started", name);
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 409, ..
            }) => {
                // 既に存在する場合は再起動
                self.docker
                    .restart_container(
                        &container_name,
                        None::<bollard::query_parameters::RestartContainerOptions>,
                    )
                    .await?;
                info!("Service {} restarted", name);
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    /// 指定されたステージを停止・削除する
    pub async fn down(&self, flow: &Flow, stage_name: &str, remove: bool) -> Result<()> {
        let stage = flow
            .stages
            .get(stage_name)
            .ok_or_else(|| anyhow::anyhow!("Stage '{}' not found", stage_name))?;

        for service_name in &stage.services {
            let container_name = format!("{}-{}-{}", flow.name, stage_name, service_name);

            info!("Stopping container: {}", container_name);
            let _ = self
                .docker
                .stop_container(
                    &container_name,
                    None::<bollard::query_parameters::StopContainerOptions>,
                )
                .await;

            if remove {
                info!("Removing container: {}", container_name);
                let _ = self
                    .docker
                    .remove_container(
                        &container_name,
                        None::<bollard::query_parameters::RemoveContainerOptions>,
                    )
                    .await;
            }
        }

        if remove {
            let network_name = crate::get_network_name(&flow.name, stage_name);
            info!("Removing network: {}", network_name);
            let _ = self.docker.remove_network(&network_name).await;
        }

        Ok(())
    }
}

use tracing::debug;
