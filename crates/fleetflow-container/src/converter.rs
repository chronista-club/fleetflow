//! FlowConfig から Docker API パラメータへの変換

use bollard::container::{Config, CreateContainerOptions, NetworkingConfig};
use bollard::models::{EndpointSettings, HostConfig, PortBinding};
use fleetflow_atom::{Flow, Service};
use std::collections::HashMap;

/// ネットワーク名を生成
pub fn get_network_name(project_name: &str, stage_name: &str) -> String {
    format!("{}-{}", project_name, stage_name)
}

/// FlowConfigのServiceをDockerのコンテナ設定に変換
pub fn service_to_container_config(
    service_name: &str,
    service: &Service,
    stage_name: &str,
    project_name: &str,
) -> (Config<String>, CreateContainerOptions<String>) {
    service_to_container_config_with_network(service_name, service, stage_name, project_name, true)
}

/// FlowConfigのServiceをDockerのコンテナ設定に変換（ネットワーク設定オプション付き）
pub fn service_to_container_config_with_network(
    service_name: &str,
    service: &Service,
    stage_name: &str,
    project_name: &str,
    use_network: bool,
) -> (Config<String>, CreateContainerOptions<String>) {
    // イメージ名の決定
    // 1. imageとversionの両方が指定されている場合は "image:version"
    // 2. imageのみでタグが含まれている場合（":"を含む）はそのまま使用
    // 3. imageのみでタグがない場合は "image:latest"
    // 4. versionのみの場合は "service_name:version"
    // 5. どちらもない場合は "service_name:latest"
    let image = match (&service.image, &service.version) {
        (Some(img), Some(ver)) => format!("{}:{}", img, ver),
        (Some(img), None) => {
            if img.contains(':') {
                img.clone()
            } else {
                format!("{}:latest", img)
            }
        }
        (None, Some(ver)) => format!("{}:{}", service_name, ver),
        (None, None) => format!("{}:latest", service_name),
    };

    // 環境変数の設定
    let env: Vec<String> = service
        .environment
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();

    // ポートバインディングの設定
    let mut port_bindings = HashMap::new();
    let mut exposed_ports = HashMap::new();

    for port in &service.ports {
        let container_port = format!(
            "{}/{}",
            port.container,
            if port.protocol == fleetflow_atom::Protocol::Udp {
                "udp"
            } else {
                "tcp"
            }
        );

        // ポート公開設定
        exposed_ports.insert(container_port.clone(), HashMap::new());

        // ホストポートバインディング
        let host_ip = port.host_ip.as_deref().unwrap_or("0.0.0.0");
        port_bindings.insert(
            container_port,
            Some(vec![PortBinding {
                host_ip: Some(host_ip.to_string()),
                host_port: Some(port.host.to_string()),
            }]),
        );
    }

    // ボリュームバインディング
    let binds: Vec<String> = service
        .volumes
        .iter()
        .map(|v| {
            let mode = if v.read_only { "ro" } else { "rw" };
            // 相対パスの場合は絶対パスに変換
            let host_path = if v.host.is_relative() {
                std::env::current_dir()
                    .unwrap_or_else(|_| v.host.clone())
                    .join(&v.host)
            } else {
                v.host.clone()
            };
            format!("{}:{}:{}", host_path.display(), v.container.display(), mode)
        })
        .collect();

    // ネットワーク名
    let network_name = get_network_name(project_name, stage_name);

    // HostConfig設定（ネットワーク対応 #14）
    let host_config = Some(HostConfig {
        port_bindings: Some(port_bindings),
        binds: Some(binds),
        network_mode: if use_network {
            Some(network_name.clone())
        } else {
            None
        },
        ..Default::default()
    });

    // ラベル設定（OrbStackグループ化対応）
    let mut labels = HashMap::new();
    labels.insert(
        "com.docker.compose.project".to_string(),
        format!("{}-{}", project_name, stage_name),
    );
    labels.insert(
        "com.docker.compose.service".to_string(),
        service_name.to_string(),
    );
    labels.insert("fleetflow.project".to_string(), project_name.to_string());
    labels.insert("fleetflow.stage".to_string(), stage_name.to_string());
    labels.insert("fleetflow.service".to_string(), service_name.to_string());

    // ネットワーク設定（サービス名でエイリアス #14）
    let networking_config = if use_network {
        let mut endpoints = HashMap::new();
        endpoints.insert(
            network_name,
            EndpointSettings {
                aliases: Some(vec![service_name.to_string()]),
                ..Default::default()
            },
        );
        Some(NetworkingConfig {
            endpoints_config: endpoints,
        })
    } else {
        None
    };

    // コンテナ設定
    let config = Config {
        image: Some(image),
        env: Some(env),
        exposed_ports: Some(exposed_ports),
        host_config,
        labels: Some(labels),
        cmd: service.command.as_ref().map(|c| {
            // コマンドをスペースで分割
            c.split_whitespace().map(String::from).collect()
        }),
        networking_config,
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: format!("{}-{}-{}", project_name, stage_name, service_name),
        platform: None,
    };

    (config, options)
}

/// ステージに含まれるサービスのリストを取得
pub fn get_stage_services(flow: &Flow, stage_name: &str) -> Result<Vec<String>, String> {
    flow.stages
        .get(stage_name)
        .map(|stage| stage.services.clone())
        .ok_or_else(|| format!("Stage '{}' not found", stage_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetflow_atom::{Port, Protocol, Service, Stage, Volume};
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn test_service_to_container_config_basic() {
        let service = Service {
            image: Some("postgres".to_string()),
            version: Some("16".to_string()),
            ..Default::default()
        };

        let (config, options) = service_to_container_config("postgres", &service, "local", "vantage");

        assert_eq!(config.image, Some("postgres:16".to_string()));
        assert_eq!(options.name, "vantage-local-postgres");
    }

    #[test]
    fn test_service_to_container_config_default_image() {
        let service = Service::default();

        let (config, _) = service_to_container_config("redis", &service, "local", "test");

        // imageが未指定の場合は"サービス名:latest"になる
        assert_eq!(config.image, Some("redis:latest".to_string()));
    }

    #[test]
    fn test_service_to_container_config_with_environment() {
        let mut environment = HashMap::new();
        environment.insert(
            "DATABASE_URL".to_string(),
            "postgres://localhost".to_string(),
        );
        environment.insert("DEBUG".to_string(), "true".to_string());

        let service = Service {
            environment,
            ..Default::default()
        };

        let (config, _) = service_to_container_config("api", &service, "local", "test");

        let env = config.env.unwrap();
        assert!(env.contains(&"DATABASE_URL=postgres://localhost".to_string()));
        assert!(env.contains(&"DEBUG=true".to_string()));
    }

    #[test]
    fn test_service_to_container_config_with_ports() {
        let ports = vec![
            Port {
                host: 8080,
                container: 3000,
                protocol: Protocol::Tcp,
                host_ip: None,
            },
            Port {
                host: 5432,
                container: 5432,
                protocol: Protocol::Tcp,
                host_ip: Some("127.0.0.1".to_string()),
            },
        ];

        let service = Service {
            ports,
            ..Default::default()
        };

        let (config, _) = service_to_container_config("web", &service, "local", "test");

        let exposed_ports = config.exposed_ports.unwrap();
        assert!(exposed_ports.contains_key("3000/tcp"));
        assert!(exposed_ports.contains_key("5432/tcp"));

        let host_config = config.host_config.unwrap();
        let port_bindings = host_config.port_bindings.unwrap();

        let binding_3000 = port_bindings.get("3000/tcp").unwrap().as_ref().unwrap();
        assert_eq!(binding_3000[0].host_port, Some("8080".to_string()));
        assert_eq!(binding_3000[0].host_ip, Some("0.0.0.0".to_string()));

        let binding_5432 = port_bindings.get("5432/tcp").unwrap().as_ref().unwrap();
        assert_eq!(binding_5432[0].host_ip, Some("127.0.0.1".to_string()));
    }

    #[test]
    fn test_service_to_container_config_with_udp_port() {
        let ports = vec![Port {
            host: 53,
            container: 53,
            protocol: Protocol::Udp,
            host_ip: None,
        }];

        let service = Service {
            ports,
            ..Default::default()
        };

        let (config, _) = service_to_container_config("dns", &service, "local", "test");

        let exposed_ports = config.exposed_ports.unwrap();
        assert!(exposed_ports.contains_key("53/udp"));
    }

    #[test]
    fn test_service_to_container_config_with_volumes() {
        let volumes = vec![
            Volume {
                host: PathBuf::from("/data"),
                container: PathBuf::from("/var/lib/data"),
                read_only: false,
            },
            Volume {
                host: PathBuf::from("/config"),
                container: PathBuf::from("/etc/config"),
                read_only: true,
            },
        ];

        let service = Service {
            volumes,
            ..Default::default()
        };

        let (config, _) = service_to_container_config("db", &service, "local", "test");

        let host_config = config.host_config.unwrap();
        let binds = host_config.binds.unwrap();

        assert_eq!(binds.len(), 2);
        assert!(binds[0].contains("/data:/var/lib/data:rw"));
        assert!(binds[1].contains("/config:/etc/config:ro"));
    }

    #[test]
    fn test_service_to_container_config_with_command() {
        let service = Service {
            command: Some("start --user root --pass root".to_string()),
            ..Default::default()
        };

        let (config, _) = service_to_container_config("db", &service, "local", "test");

        let cmd = config.cmd.unwrap();
        assert_eq!(cmd, vec!["start", "--user", "root", "--pass", "root"]);
    }

    #[test]
    fn test_get_stage_services() {
        let mut services = HashMap::new();
        services.insert("api".to_string(), Service::default());
        services.insert("db".to_string(), Service::default());

        let mut stages = HashMap::new();
        stages.insert(
            "local".to_string(),
            Stage {
                services: vec!["api".to_string(), "db".to_string()],
                servers: vec![],
                variables: HashMap::new(),
            },
        );

        let flow = Flow {
            name: "test".to_string(),
            services,
            stages,
            providers: HashMap::new(),
            servers: HashMap::new(),
        };

        let result = get_stage_services(&flow, "local").unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"api".to_string()));
        assert!(result.contains(&"db".to_string()));
    }

    #[test]
    fn test_get_stage_services_not_found() {
        let flow = Flow {
            name: "test".to_string(),
            services: HashMap::new(),
            stages: HashMap::new(),
            providers: HashMap::new(),
            servers: HashMap::new(),
        };

        let result = get_stage_services(&flow, "prod");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Stage 'prod' not found");
    }

    #[test]
    fn test_container_name_format() {
        let service = Service::default();
        let (_, options) = service_to_container_config("my-service", &service, "dev", "myapp");

        assert_eq!(options.name, "myapp-dev-my-service");
    }

    #[test]
    fn test_orbstack_labels_generation() {
        let service = Service::default();
        let (config, _) = service_to_container_config("postgres", &service, "local", "vantage");

        let labels = config.labels.unwrap();

        // OrbStackグループ化用ラベル
        assert_eq!(
            labels.get("com.docker.compose.project"),
            Some(&"vantage-local".to_string())
        );
        assert_eq!(
            labels.get("com.docker.compose.service"),
            Some(&"postgres".to_string())
        );

        // FleetFlowメタデータラベル
        assert_eq!(
            labels.get("fleetflow.project"),
            Some(&"vantage".to_string())
        );
        assert_eq!(
            labels.get("fleetflow.stage"),
            Some(&"local".to_string())
        );
        assert_eq!(
            labels.get("fleetflow.service"),
            Some(&"postgres".to_string())
        );

        // 全部で5つのラベルがあることを確認
        assert_eq!(labels.len(), 5);
    }

    #[test]
    fn test_orbstack_labels_with_different_stages() {
        let service = Service::default();

        // localステージ
        let (config_local, _) =
            service_to_container_config("api", &service, "local", "myapp");
        let labels_local = config_local.labels.unwrap();
        assert_eq!(
            labels_local.get("com.docker.compose.project"),
            Some(&"myapp-local".to_string())
        );

        // prodステージ
        let (config_prod, _) = service_to_container_config("api", &service, "prod", "myapp");
        let labels_prod = config_prod.labels.unwrap();
        assert_eq!(
            labels_prod.get("com.docker.compose.project"),
            Some(&"myapp-prod".to_string())
        );
        assert_eq!(
            labels_prod.get("fleetflow.stage"),
            Some(&"prod".to_string())
        );
    }

    #[test]
    fn test_orbstack_labels_with_multiple_projects() {
        let service = Service::default();

        // プロジェクトA
        let (config_a, _) = service_to_container_config("db", &service, "local", "project-a");
        let labels_a = config_a.labels.unwrap();
        assert_eq!(
            labels_a.get("com.docker.compose.project"),
            Some(&"project-a-local".to_string())
        );
        assert_eq!(
            labels_a.get("fleetflow.project"),
            Some(&"project-a".to_string())
        );

        // プロジェクトB
        let (config_b, _) = service_to_container_config("db", &service, "local", "project-b");
        let labels_b = config_b.labels.unwrap();
        assert_eq!(
            labels_b.get("com.docker.compose.project"),
            Some(&"project-b-local".to_string())
        );
        assert_eq!(
            labels_b.get("fleetflow.project"),
            Some(&"project-b".to_string())
        );
    }
}
