//! FlowConfig から Docker API パラメータへの変換

use bollard::container::{Config, CreateContainerOptions};
use bollard::models::{HostConfig, PortBinding};
use flow_atom::{FlowConfig, Service};
use std::collections::HashMap;

/// FlowConfigのServiceをDockerのコンテナ設定に変換
pub fn service_to_container_config(
    service_name: &str,
    service: &Service,
) -> (Config<String>, CreateContainerOptions<String>) {
    let image = service
        .image
        .as_ref()
        .cloned()
        .unwrap_or_else(|| format!("{}:latest", service_name));

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
            if port.protocol == flow_atom::Protocol::Udp {
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

    // HostConfig設定
    let host_config = Some(HostConfig {
        port_bindings: Some(port_bindings),
        binds: Some(binds),
        ..Default::default()
    });

    // コンテナ設定
    let config = Config {
        image: Some(image),
        env: Some(env),
        exposed_ports: Some(exposed_ports),
        host_config,
        cmd: service.command.as_ref().map(|c| {
            // コマンドをスペースで分割
            c.split_whitespace().map(String::from).collect()
        }),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: format!("flow-{}", service_name),
        platform: None,
    };

    (config, options)
}

/// ステージに含まれるサービスのリストを取得
pub fn get_stage_services(config: &FlowConfig, stage_name: &str) -> Result<Vec<String>, String> {
    config
        .stages
        .get(stage_name)
        .map(|stage| stage.services.clone())
        .ok_or_else(|| format!("Stage '{}' not found", stage_name))
}
