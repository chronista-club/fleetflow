//! DeployEngine — コンテナデプロイの実行エンジン
//!
//! CLI（ローカル）と CP（リモート）の両方から利用できるデプロイ実行エンジン。
//! Docker API (bollard) を直接操作し、5 ステップのデプロイフローを実行する。

use std::collections::HashMap;

use bollard::Docker;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};

use crate::converter;
use fleetflow_core::Flow;

/// デプロイリクエスト（JSON シリアライズ可能 → Unison で送受信）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    pub flow: Flow,
    pub stage_name: String,
    pub target_services: Vec<String>,
    #[serde(default)]
    pub no_pull: bool,
    #[serde(default)]
    pub no_prune: bool,
}

/// 進捗イベント（CLI は表示、CP はログ記録）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DeployEvent {
    StepStarted {
        step: u8,
        total: u8,
        description: String,
    },
    ServiceProgress {
        service: String,
        action: String,
    },
    StepCompleted {
        step: u8,
    },
    Completed {
        services_deployed: Vec<String>,
    },
    Error {
        message: String,
    },
}

/// デプロイ結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub success: bool,
    pub services_deployed: Vec<String>,
    pub log: Vec<String>,
}

/// デプロイ実行エンジン
pub struct DeployEngine {
    docker: Docker,
}

/// 依存関係を考慮してサービスをソート
///
/// depends_on が空のサービスを先に、依存があるサービスを後に配置する。
pub fn order_by_dependencies(services: &[String], flow: &Flow) -> Vec<String> {
    let mut ordered: Vec<String> = Vec::new();
    let mut remaining: Vec<String> = Vec::new();

    for name in services {
        if let Some(svc) = flow.services.get(name) {
            if svc.depends_on.is_empty() {
                ordered.push(name.clone());
            } else {
                remaining.push(name.clone());
            }
        } else {
            ordered.push(name.clone());
        }
    }

    ordered.extend(remaining);
    ordered
}

impl DeployEngine {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// デプロイを実行
    ///
    /// 5 ステップ:
    /// 1. 既存コンテナの停止・削除
    /// 2. イメージの pull
    /// 3. ネットワーク作成
    /// 4. コンテナ作成・起動（依存順）
    /// 5. 不要イメージ・キャッシュの削除
    pub async fn execute(
        &self,
        request: &DeployRequest,
        on_event: impl Fn(DeployEvent),
    ) -> anyhow::Result<DeployResult> {
        let flow = &request.flow;
        let stage_name = &request.stage_name;
        let mut log: Vec<String> = Vec::new();

        // Step 1: 既存コンテナの停止・削除
        on_event(DeployEvent::StepStarted {
            step: 1,
            total: 5,
            description: "既存コンテナを停止・削除中...".into(),
        });
        self.stop_and_remove(
            flow,
            stage_name,
            &request.target_services,
            &on_event,
            &mut log,
        )
        .await;
        on_event(DeployEvent::StepCompleted { step: 1 });

        // Step 2: イメージの pull
        on_event(DeployEvent::StepStarted {
            step: 2,
            total: 5,
            description: if request.no_pull {
                "イメージ pull をスキップ（--no-pull 指定）".into()
            } else {
                "最新イメージをダウンロード中...".into()
            },
        });
        if !request.no_pull {
            self.pull_images(flow, &request.target_services, &on_event, &mut log)
                .await;
        }
        on_event(DeployEvent::StepCompleted { step: 2 });

        // Step 3: ネットワーク作成
        let network_name = converter::get_network_name(&flow.name, stage_name);
        on_event(DeployEvent::StepStarted {
            step: 3,
            total: 5,
            description: format!("ネットワーク準備中: {}", network_name),
        });
        self.ensure_network(&network_name, &mut log).await?;
        on_event(DeployEvent::StepCompleted { step: 3 });

        // Step 4: コンテナ作成・起動（依存順）
        on_event(DeployEvent::StepStarted {
            step: 4,
            total: 5,
            description: "コンテナを作成・起動中...".into(),
        });
        let ordered = order_by_dependencies(&request.target_services, flow);
        self.create_and_start(
            flow,
            stage_name,
            &ordered,
            request.no_pull,
            &on_event,
            &mut log,
        )
        .await?;
        on_event(DeployEvent::StepCompleted { step: 4 });

        // Step 5: 不要イメージ・キャッシュ削除
        on_event(DeployEvent::StepStarted {
            step: 5,
            total: 5,
            description: if request.no_prune {
                "prune をスキップ（--no-prune 指定）".into()
            } else {
                "不要イメージ・ビルドキャッシュを削除中...".into()
            },
        });
        if !request.no_prune {
            self.prune(&on_event, &mut log).await;
        }
        on_event(DeployEvent::StepCompleted { step: 5 });

        let services_deployed = request.target_services.clone();
        on_event(DeployEvent::Completed {
            services_deployed: services_deployed.clone(),
        });

        Ok(DeployResult {
            success: true,
            services_deployed,
            log,
        })
    }

    /// Step 1: 既存コンテナの停止・削除
    async fn stop_and_remove(
        &self,
        flow: &Flow,
        stage_name: &str,
        services: &[String],
        on_event: &impl Fn(DeployEvent),
        log: &mut Vec<String>,
    ) {
        for service_name in services {
            let container_name = format!("{}-{}-{}", flow.name, stage_name, service_name);

            // 停止
            match self
                .docker
                .stop_container(
                    &container_name,
                    None::<bollard::query_parameters::StopContainerOptions>,
                )
                .await
            {
                Ok(_) => {
                    on_event(DeployEvent::ServiceProgress {
                        service: service_name.clone(),
                        action: "stopped".into(),
                    });
                    log.push(format!("{}: stopped", service_name));
                }
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => {
                    log.push(format!("{}: no container", service_name));
                }
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 304, ..
                }) => {
                    log.push(format!("{}: already stopped", service_name));
                }
                Err(e) => {
                    log.push(format!("{}: stop error: {}", service_name, e));
                }
            }

            // 削除（強制）
            match self
                .docker
                .remove_container(
                    &container_name,
                    Some(bollard::query_parameters::RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
            {
                Ok(_) => {
                    on_event(DeployEvent::ServiceProgress {
                        service: service_name.clone(),
                        action: "removed".into(),
                    });
                    log.push(format!("{}: removed", service_name));
                }
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => {}
                Err(e) => {
                    log.push(format!("{}: remove error: {}", service_name, e));
                }
            }
        }
    }

    /// Step 2: イメージの pull
    async fn pull_images(
        &self,
        flow: &Flow,
        services: &[String],
        on_event: &impl Fn(DeployEvent),
        log: &mut Vec<String>,
    ) {
        for service_name in services {
            if let Some(svc) = flow.services.get(service_name)
                && let Some(image) = &svc.image
            {
                on_event(DeployEvent::ServiceProgress {
                    service: service_name.clone(),
                    action: format!("pulling {}", image),
                });

                match self.pull_image(image).await {
                    Ok(_) => {
                        log.push(format!("{}: pulled {}", service_name, image));
                    }
                    Err(e) => {
                        log.push(format!("{}: pull error: {}", service_name, e));
                    }
                }
            }
        }
    }

    /// 単一イメージを pull
    async fn pull_image(&self, image: &str) -> anyhow::Result<()> {
        let (image_name, tag) = if let Some((name, tag)) = image.split_once(':') {
            (name, tag)
        } else {
            (image, "latest")
        };

        let options = bollard::query_parameters::CreateImageOptions {
            from_image: Some(image_name.to_string()),
            tag: Some(tag.to_string()),
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);

        while let Some(info) = stream.next().await {
            match info {
                Ok(_) => {}
                Err(e) => {
                    return Err(anyhow::anyhow!("イメージ pull 失敗: {}", e));
                }
            }
        }

        Ok(())
    }

    /// Step 3: ネットワーク作成
    async fn ensure_network(
        &self,
        network_name: &str,
        log: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        let network_config = bollard::models::NetworkCreateRequest {
            name: network_name.to_string(),
            driver: Some("bridge".to_string()),
            ..Default::default()
        };

        match self.docker.create_network(network_config).await {
            Ok(_) => {
                log.push(format!("network created: {}", network_name));
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 409, ..
            }) => {
                log.push(format!("network exists: {}", network_name));
            }
            Err(e) => {
                return Err(anyhow::anyhow!("ネットワーク作成エラー: {}", e));
            }
        }

        Ok(())
    }

    /// Step 4: コンテナ作成・起動
    async fn create_and_start(
        &self,
        flow: &Flow,
        stage_name: &str,
        ordered_services: &[String],
        no_pull: bool,
        on_event: &impl Fn(DeployEvent),
        log: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        for service_name in ordered_services {
            let service_def = match flow.services.get(service_name) {
                Some(s) => s,
                None => {
                    log.push(format!("{}: definition not found, skipping", service_name));
                    continue;
                }
            };

            on_event(DeployEvent::ServiceProgress {
                service: service_name.clone(),
                action: "creating".into(),
            });

            let (container_config, create_options) = converter::service_to_container_config(
                service_name,
                service_def,
                stage_name,
                &flow.name,
            );

            let image = container_config.image.as_ref().ok_or_else(|| {
                anyhow::anyhow!("サービス '{}' のイメージ設定が見つかりません", service_name)
            })?;

            // --no-pull でもローカルにイメージがなければ pull
            if no_pull {
                match self.docker.inspect_image(image).await {
                    Ok(_) => {}
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        ..
                    }) => {
                        self.pull_image(image).await?;
                        log.push(format!("{}: auto-pulled {}", service_name, image));
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            // コンテナ作成
            self.docker
                .create_container(Some(create_options), container_config)
                .await
                .map_err(|e| anyhow::anyhow!("コンテナ作成エラー ({}): {}", service_name, e))?;
            log.push(format!("{}: created", service_name));

            // 依存サービスの待機
            if let Some(wait_config) = &service_def.wait_for
                && !service_def.depends_on.is_empty()
            {
                for dep_service in &service_def.depends_on {
                    let dep_container = format!("{}-{}-{}", flow.name, stage_name, dep_service);
                    match crate::wait_for_service(&self.docker, &dep_container, wait_config).await {
                        Ok(_) => {
                            log.push(format!(
                                "{}: dependency {} ready",
                                service_name, dep_service
                            ));
                        }
                        Err(e) => {
                            log.push(format!(
                                "{}: dependency {} wait error: {}",
                                service_name, dep_service, e
                            ));
                        }
                    }
                }
            }

            // コンテナ起動
            let container_name = format!("{}-{}-{}", flow.name, stage_name, service_name);
            self.docker
                .start_container(
                    &container_name,
                    None::<bollard::query_parameters::StartContainerOptions>,
                )
                .await
                .map_err(|e| anyhow::anyhow!("起動エラー ({}): {}", service_name, e))?;

            on_event(DeployEvent::ServiceProgress {
                service: service_name.clone(),
                action: "started".into(),
            });
            log.push(format!("{}: started", service_name));
        }

        Ok(())
    }

    /// Step 5: 不要イメージ・キャッシュ削除
    async fn prune(&self, on_event: &impl Fn(DeployEvent), log: &mut Vec<String>) {
        // 1 週間以上古い未使用イメージを削除
        let mut image_filters = HashMap::new();
        image_filters.insert("until".to_string(), vec!["168h".to_string()]);
        image_filters.insert("dangling".to_string(), vec!["true".to_string()]);

        let prune_opts = bollard::query_parameters::PruneImagesOptions {
            filters: Some(image_filters),
        };

        match self.docker.prune_images(Some(prune_opts)).await {
            Ok(result) => {
                let deleted_count = result.images_deleted.as_ref().map(|v| v.len()).unwrap_or(0);
                let reclaimed = result.space_reclaimed.unwrap_or(0);
                if deleted_count > 0 || reclaimed > 0 {
                    let reclaimed_mb = reclaimed as f64 / 1_048_576.0;
                    let msg = format!(
                        "pruned {} images ({:.1}MB reclaimed)",
                        deleted_count, reclaimed_mb
                    );
                    on_event(DeployEvent::ServiceProgress {
                        service: "prune".into(),
                        action: msg.clone(),
                    });
                    log.push(msg);
                }
            }
            Err(e) => {
                log.push(format!("image prune error: {}", e));
            }
        }

        // ビルドキャッシュの削除
        let mut build_filters = HashMap::new();
        build_filters.insert("until".to_string(), vec!["168h".to_string()]);

        let build_prune_opts = bollard::query_parameters::PruneBuildOptions {
            filters: Some(build_filters),
            ..Default::default()
        };

        match self.docker.prune_build(Some(build_prune_opts)).await {
            Ok(result) => {
                let reclaimed = result.space_reclaimed.unwrap_or(0);
                if reclaimed > 0 {
                    let reclaimed_mb = reclaimed as f64 / 1_048_576.0;
                    log.push(format!(
                        "build cache pruned ({:.1}MB reclaimed)",
                        reclaimed_mb
                    ));
                }
            }
            Err(e) => {
                log.push(format!("build cache prune error: {}", e));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetflow_core::{Service, Stage};

    fn make_test_flow(services: Vec<(&str, Service)>, stage_services: Vec<&str>) -> Flow {
        let mut svc_map = std::collections::HashMap::new();
        for (name, svc) in services {
            svc_map.insert(name.to_string(), svc);
        }
        let mut stages = std::collections::HashMap::new();
        stages.insert(
            "local".to_string(),
            Stage {
                services: stage_services.iter().map(|s| s.to_string()).collect(),
                servers: vec![],
                variables: std::collections::HashMap::new(),
                registry: None,
            },
        );
        Flow {
            name: "test-project".to_string(),
            services: svc_map,
            stages,
            providers: std::collections::HashMap::new(),
            servers: std::collections::HashMap::new(),
            registry: None,
            variables: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_deploy_request_serialization() {
        let flow = make_test_flow(
            vec![(
                "web",
                Service {
                    image: Some("node:20".into()),
                    ..Default::default()
                },
            )],
            vec!["web"],
        );
        let request = DeployRequest {
            flow,
            stage_name: "local".into(),
            target_services: vec!["web".into()],
            no_pull: false,
            no_prune: false,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: DeployRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.stage_name, "local");
        assert_eq!(deserialized.target_services, vec!["web"]);
        assert!(!deserialized.no_pull);
        assert!(!deserialized.no_prune);
    }

    #[test]
    fn test_deploy_event_variants() {
        let events = vec![
            DeployEvent::StepStarted {
                step: 1,
                total: 5,
                description: "test".into(),
            },
            DeployEvent::ServiceProgress {
                service: "web".into(),
                action: "pulling".into(),
            },
            DeployEvent::StepCompleted { step: 1 },
            DeployEvent::Completed {
                services_deployed: vec!["web".into()],
            },
            DeployEvent::Error {
                message: "oops".into(),
            },
        ];

        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let _: DeployEvent = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_order_by_dependencies_no_deps() {
        let flow = make_test_flow(
            vec![("web", Service::default()), ("db", Service::default())],
            vec!["web", "db"],
        );
        let services = vec!["web".into(), "db".into()];
        let ordered = order_by_dependencies(&services, &flow);
        // 両方 depends_on なし → 元の順序を維持
        assert_eq!(ordered, vec!["web", "db"]);
    }

    #[test]
    fn test_order_by_dependencies_with_deps() {
        let flow = make_test_flow(
            vec![
                (
                    "web",
                    Service {
                        depends_on: vec!["db".into()],
                        ..Default::default()
                    },
                ),
                ("db", Service::default()),
            ],
            vec!["web", "db"],
        );
        let services = vec!["web".into(), "db".into()];
        let ordered = order_by_dependencies(&services, &flow);
        // db が先、web（depends_on あり）が後
        assert_eq!(ordered, vec!["db", "web"]);
    }

    #[test]
    fn test_order_by_dependencies_mixed() {
        let flow = make_test_flow(
            vec![
                (
                    "api",
                    Service {
                        depends_on: vec!["db".into()],
                        ..Default::default()
                    },
                ),
                ("db", Service::default()),
                ("redis", Service::default()),
                (
                    "worker",
                    Service {
                        depends_on: vec!["redis".into(), "db".into()],
                        ..Default::default()
                    },
                ),
            ],
            vec!["api", "db", "redis", "worker"],
        );
        let services = vec!["api".into(), "db".into(), "redis".into(), "worker".into()];
        let ordered = order_by_dependencies(&services, &flow);
        // db, redis（依存なし）が先、api, worker（依存あり）が後
        assert_eq!(ordered[0], "db");
        assert_eq!(ordered[1], "redis");
        assert!(ordered[2..].contains(&"api".to_string()));
        assert!(ordered[2..].contains(&"worker".to_string()));
    }

    #[test]
    fn test_deploy_request_from_flow() {
        let flow = make_test_flow(
            vec![
                (
                    "api",
                    Service {
                        image: Some("myapp:1.0".into()),
                        ..Default::default()
                    },
                ),
                (
                    "db",
                    Service {
                        image: Some("postgres:16".into()),
                        ..Default::default()
                    },
                ),
            ],
            vec!["api", "db"],
        );

        let request = DeployRequest {
            flow: flow.clone(),
            stage_name: "local".into(),
            target_services: vec!["api".into(), "db".into()],
            no_pull: true,
            no_prune: true,
        };

        assert_eq!(request.flow.name, "test-project");
        assert_eq!(request.flow.services.len(), 2);
        assert!(request.no_pull);
        assert!(request.no_prune);
        assert_eq!(
            request.flow.services.get("api").unwrap().image,
            Some("myapp:1.0".into())
        );
    }

    #[test]
    fn test_deploy_result_serialization() {
        let result = DeployResult {
            success: true,
            services_deployed: vec!["web".into(), "db".into()],
            log: vec!["web: started".into(), "db: started".into()],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: DeployResult = serde_json::from_str(&json).unwrap();

        assert!(deserialized.success);
        assert_eq!(deserialized.services_deployed, vec!["web", "db"]);
        assert_eq!(deserialized.log.len(), 2);
    }
}
