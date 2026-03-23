//! サービスノードのパース

use super::port::parse_port;
use super::volume::parse_volume;
use crate::error::{FlowError, Result};
use crate::model::{BuildConfig, DeployConfig, RestartPolicy, Service, ServiceType, WaitConfig};
use kdl::{KdlDocument, KdlNode};
use std::path::PathBuf;

/// service ノードをパース
pub fn parse_service(node: &KdlNode) -> Result<(String, Service)> {
    let name = node
        .entries()
        .first()
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| FlowError::InvalidConfig("service requires a name".to_string()))?
        .to_string();

    let mut service = Service::default();

    // ノードのプロパティを読み取り（type="static" command="..." 等）
    for entry in node.entries() {
        if let Some(key) = entry.name() {
            match key.value() {
                "type" | "service_type" => {
                    service.service_type = entry.value().as_string().and_then(ServiceType::parse);
                }
                "command" => {
                    service.command = entry.value().as_string().map(|s| s.to_string());
                }
                "image" => {
                    service.image = entry.value().as_string().map(|s| s.to_string());
                }
                "version" => {
                    service.version = entry.value().as_string().map(|s| s.to_string());
                }
                "restart" => {
                    service.restart = entry.value().as_string().and_then(RestartPolicy::parse);
                }
                "registry" => {
                    service.registry = entry.value().as_string().map(|s| s.to_string());
                }
                _ => {}
            }
        }
    }

    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "image" => {
                    service.image = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "version" => {
                    service.version = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "command" => {
                    service.command = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "ports" => {
                    if let Some(ports) = child.children() {
                        for port_node in ports.nodes() {
                            if port_node.name().value() == "port"
                                && let Some(port) = parse_port(port_node)
                            {
                                service.ports.push(port);
                            }
                        }
                    }
                }
                "port" => {
                    if let Some(port) = parse_port(child) {
                        service.ports.push(port);
                    }
                }
                // env と environment 両方をサポート (#12)
                "environment" | "env" => {
                    if let Some(envs) = child.children() {
                        for env_node in envs.nodes() {
                            let key = env_node.name().value().to_string();
                            let value = env_node
                                .entries()
                                .first()
                                .and_then(|e| e.value().as_string())
                                .unwrap_or("")
                                .to_string();
                            service.environment.insert(key, value);
                        }
                    } else {
                        // 子ノードがない場合は、フラットな env "KEY=VALUE" 形式をサポート
                        if let Some(val) =
                            child.entries().first().and_then(|e| e.value().as_string())
                            && let Some((k, v)) = val.split_once('=')
                        {
                            service
                                .environment
                                .insert(k.trim().to_string(), v.trim().to_string());
                        }
                    }
                }
                "volumes" => {
                    if let Some(vols) = child.children() {
                        for vol_node in vols.nodes() {
                            if vol_node.name().value() == "volume"
                                && let Some(volume) = parse_volume(vol_node)
                            {
                                service.volumes.push(volume);
                            }
                        }
                    }
                }
                "depends_on" => {
                    service.depends_on = child
                        .entries()
                        .iter()
                        .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
                        .collect();
                }
                // ビルド関連フィールド（フラット記法）
                "dockerfile" => {
                    if let Some(path) = child.entries().first().and_then(|e| e.value().as_string())
                    {
                        service
                            .build
                            .get_or_insert_with(Default::default)
                            .dockerfile = Some(PathBuf::from(path));
                    }
                }
                "context" => {
                    if let Some(path) = child.entries().first().and_then(|e| e.value().as_string())
                    {
                        service.build.get_or_insert_with(Default::default).context =
                            Some(PathBuf::from(path));
                    }
                }
                "target" => {
                    if let Some(target) =
                        child.entries().first().and_then(|e| e.value().as_string())
                    {
                        service.build.get_or_insert_with(Default::default).target =
                            Some(target.to_string());
                    }
                }
                "build_args" => {
                    if let Some(args) = child.children() {
                        let build_config = service.build.get_or_insert_with(Default::default);
                        for arg_node in args.nodes() {
                            let key = arg_node.name().value().to_string();
                            let value = arg_node
                                .entries()
                                .first()
                                .and_then(|e| e.value().as_string())
                                .unwrap_or("")
                                .to_string();
                            build_config.args.insert(key, value);
                        }
                    }
                }
                "image_tag" => {
                    if let Some(tag) = child.entries().first().and_then(|e| e.value().as_string()) {
                        service.build.get_or_insert_with(Default::default).image_tag =
                            Some(tag.to_string());
                    }
                }
                // ネストしたbuildブロック
                "build" => {
                    if let Some(build_children) = child.children() {
                        service.build = Some(parse_build_config(build_children));
                    }
                }
                // ヘルスチェックブロック
                "healthcheck" => {
                    if let Some(healthcheck_children) = child.children() {
                        service.healthcheck = Some(parse_healthcheck(healthcheck_children));
                    }
                }
                // 再起動ポリシー
                "restart" => {
                    if let Some(policy_str) =
                        child.entries().first().and_then(|e| e.value().as_string())
                    {
                        service.restart = RestartPolicy::parse(policy_str);
                    }
                }
                // 依存サービス待機設定（exponential backoff）
                "wait_for" => {
                    if let Some(wait_children) = child.children() {
                        service.wait_for = Some(parse_wait_config(wait_children));
                    } else {
                        // 子ノードがなければデフォルト設定で有効化
                        service.wait_for = Some(WaitConfig::default());
                    }
                }
                // Readinessチェック
                "readiness" => {
                    service.readiness = Some(parse_readiness(child));
                }
                // コンテナレジストリURL
                "registry" => {
                    service.registry = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                // デプロイ先設定
                "deploy" => {
                    service.deploy = Some(parse_deploy(child));
                }
                _ => {}
            }
        }
    }

    // プロパティで設定されなかった場合、子ノードの command も読む
    // （子ノード match 内で既に処理されているので、ここでは不要）

    // 注意: イメージ名の自動推測は parse_kdl_string() で全てのマージが完了した後に行う
    // ここでは行わない（マージ時に上書きされてしまうため）

    Ok((name, service))
}

/// buildブロックをパース（ネスト記法用）
pub fn parse_build_config(doc: &KdlDocument) -> BuildConfig {
    let mut config = BuildConfig::default();

    for node in doc.nodes() {
        match node.name().value() {
            "dockerfile" => {
                if let Some(path) = node.entries().first().and_then(|e| e.value().as_string()) {
                    config.dockerfile = Some(PathBuf::from(path));
                }
            }
            "context" => {
                if let Some(path) = node.entries().first().and_then(|e| e.value().as_string()) {
                    config.context = Some(PathBuf::from(path));
                }
            }
            "args" => {
                if let Some(args_children) = node.children() {
                    for arg_node in args_children.nodes() {
                        let key = arg_node.name().value().to_string();
                        let value = arg_node
                            .entries()
                            .first()
                            .and_then(|e| e.value().as_string())
                            .unwrap_or("")
                            .to_string();
                        config.args.insert(key, value);
                    }
                }
            }
            "target" => {
                if let Some(target) = node.entries().first().and_then(|e| e.value().as_string()) {
                    config.target = Some(target.to_string());
                }
            }
            "no_cache" => {
                if let Some(value) = node.entries().first().and_then(|e| e.value().as_bool()) {
                    config.no_cache = value;
                }
            }
            "image_tag" => {
                if let Some(tag) = node.entries().first().and_then(|e| e.value().as_string()) {
                    config.image_tag = Some(tag.to_string());
                }
            }
            _ => {}
        }
    }

    config
}

/// ヘルスチェックブロックをパース
pub fn parse_healthcheck(doc: &KdlDocument) -> crate::model::HealthCheck {
    use crate::model::HealthCheck;

    let mut test = Vec::new();
    let mut interval = 30;
    let mut timeout = 3;
    let mut retries = 3;
    let mut start_period = 10;

    for node in doc.nodes() {
        match node.name().value() {
            "test" => {
                // テストコマンドを配列として取得
                test = node
                    .entries()
                    .iter()
                    .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
                    .collect();
            }
            "interval" => {
                if let Some(entry) = node.entries().first()
                    && let Some(value) = entry.value().as_integer()
                {
                    interval = value as u64;
                }
            }
            "timeout" => {
                if let Some(entry) = node.entries().first()
                    && let Some(value) = entry.value().as_integer()
                {
                    timeout = value as u64;
                }
            }
            "retries" => {
                if let Some(entry) = node.entries().first()
                    && let Some(value) = entry.value().as_integer()
                {
                    retries = value as u64;
                }
            }
            "start_period" => {
                if let Some(entry) = node.entries().first()
                    && let Some(value) = entry.value().as_integer()
                {
                    start_period = value as u64;
                }
            }
            _ => {}
        }
    }

    HealthCheck {
        test,
        interval,
        timeout,
        retries,
        start_period,
    }
}

/// wait_forブロックをパース（exponential backoff設定）
pub fn parse_wait_config(doc: &KdlDocument) -> WaitConfig {
    let mut config = WaitConfig::default();

    for node in doc.nodes() {
        match node.name().value() {
            "max_retries" => {
                if let Some(entry) = node.entries().first()
                    && let Some(value) = entry.value().as_integer()
                {
                    config.max_retries = value as u32;
                }
            }
            "initial_delay" => {
                if let Some(entry) = node.entries().first()
                    && let Some(value) = entry.value().as_integer()
                {
                    config.initial_delay_ms = value as u64;
                }
            }
            "max_delay" => {
                if let Some(entry) = node.entries().first()
                    && let Some(value) = entry.value().as_integer()
                {
                    config.max_delay_ms = value as u64;
                }
            }
            "multiplier" => {
                if let Some(entry) = node.entries().first() {
                    // 整数または浮動小数点数を受け付ける
                    if let Some(value) = entry.value().as_float() {
                        config.multiplier = value;
                    } else if let Some(value) = entry.value().as_integer() {
                        config.multiplier = value as f64;
                    }
                }
            }
            _ => {}
        }
    }

    config
}

/// readinessノードをパース
///
/// プロパティ形式とブロック形式の両方をサポート:
/// ```kdl
/// // プロパティ形式（推奨）
/// readiness path="/health" port=3000 timeout=30 interval=2
///
/// // ブロック形式
/// readiness {
///     path "/health"
///     port 3000
/// }
/// ```
pub fn parse_readiness(node: &KdlNode) -> crate::model::ReadinessCheck {
    use crate::model::ReadinessCheck;

    let mut path = "/health".to_string();
    let mut port = 0u16;
    let mut timeout = 30u64;
    let mut interval = 2u64;

    // プロパティ形式: readiness path="/health" port=3000
    for entry in node.entries() {
        if let Some(name) = entry.name() {
            match name.value() {
                "path" => {
                    if let Some(v) = entry.value().as_string() {
                        path = v.to_string();
                    }
                }
                "port" => {
                    if let Some(v) = entry.value().as_integer() {
                        port = v as u16;
                    }
                }
                "timeout" => {
                    if let Some(v) = entry.value().as_integer() {
                        timeout = v as u64;
                    }
                }
                "interval" => {
                    if let Some(v) = entry.value().as_integer() {
                        interval = v as u64;
                    }
                }
                _ => {}
            }
        }
    }

    // ブロック形式: readiness { path "/health"; port 3000 }
    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "path" => {
                    if let Some(v) = child.entries().first().and_then(|e| e.value().as_string()) {
                        path = v.to_string();
                    }
                }
                "port" => {
                    if let Some(v) = child.entries().first().and_then(|e| e.value().as_integer()) {
                        port = v as u16;
                    }
                }
                "timeout" => {
                    if let Some(v) = child.entries().first().and_then(|e| e.value().as_integer()) {
                        timeout = v as u64;
                    }
                }
                "interval" => {
                    if let Some(v) = child.entries().first().and_then(|e| e.value().as_integer()) {
                        interval = v as u64;
                    }
                }
                _ => {}
            }
        }
    }

    ReadinessCheck {
        path,
        port,
        timeout,
        interval,
    }
}

/// deploy ノードをパース
///
/// プロパティ形式:
/// ```kdl
/// deploy provider="cloudflare-pages" project="my-project" output="dist/"
/// ```
pub fn parse_deploy(node: &KdlNode) -> DeployConfig {
    let mut config = DeployConfig::default();

    // プロパティからパース
    for entry in node.entries() {
        if let Some(key) = entry.name() {
            match key.value() {
                "provider" => {
                    config.provider = entry.value().as_string().map(|s| s.to_string());
                }
                "project" => {
                    config.project = entry.value().as_string().map(|s| s.to_string());
                }
                "output" => {
                    config.output = entry.value().as_string().map(|s| s.to_string());
                }
                _ => {}
            }
        }
    }

    // 子ノードからもパース（ブロック形式対応）
    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "provider" => {
                    config.provider = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "project" => {
                    config.project = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "output" => {
                    config.output = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                _ => {}
            }
        }
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdl::KdlDocument;

    #[test]
    fn test_parse_restart_policy() {
        let kdl = r#"
            service "api" {
                image "myapp:latest"
                restart "unless-stopped"
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (name, service) = parse_service(node).unwrap();

        assert_eq!(name, "api");
        assert_eq!(service.restart, Some(RestartPolicy::UnlessStopped));
    }

    #[test]
    fn test_parse_restart_policy_always() {
        let kdl = r#"
            service "db" {
                image "postgres:16"
                restart "always"
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();
        assert_eq!(service.restart, Some(RestartPolicy::Always));
    }

    #[test]
    fn test_parse_restart_policy_on_failure() {
        let kdl = r#"
            service "worker" {
                image "worker:latest"
                restart "on-failure"
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();
        assert_eq!(service.restart, Some(RestartPolicy::OnFailure));
    }

    #[test]
    fn test_parse_restart_policy_no() {
        let kdl = r#"
            service "temp" {
                image "temp:latest"
                restart "no"
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();
        assert_eq!(service.restart, Some(RestartPolicy::No));
    }

    #[test]
    fn test_parse_service_no_restart() {
        let kdl = r#"
            service "simple" {
                image "simple:latest"
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();
        assert_eq!(service.restart, None);
    }

    #[test]
    fn test_parse_healthcheck_defaults() {
        let kdl = r#"
            healthcheck {
                test "CMD-SHELL" "curl -f http://localhost:3000/health || exit 1"
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let healthcheck = parse_healthcheck(node.children().unwrap());

        assert_eq!(
            healthcheck.test,
            vec![
                "CMD-SHELL".to_string(),
                "curl -f http://localhost:3000/health || exit 1".to_string()
            ]
        );
        assert_eq!(healthcheck.interval, 30);
        assert_eq!(healthcheck.timeout, 3);
        assert_eq!(healthcheck.retries, 3);
        assert_eq!(healthcheck.start_period, 10);
    }

    #[test]
    fn test_parse_healthcheck_custom_values() {
        let kdl = r#"
            healthcheck {
                test "CMD" "python" "healthcheck.py"
                interval 60
                timeout 10
                retries 5
                start_period 30
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let healthcheck = parse_healthcheck(node.children().unwrap());

        assert_eq!(
            healthcheck.test,
            vec![
                "CMD".to_string(),
                "python".to_string(),
                "healthcheck.py".to_string()
            ]
        );
        assert_eq!(healthcheck.interval, 60);
        assert_eq!(healthcheck.timeout, 10);
        assert_eq!(healthcheck.retries, 5);
        assert_eq!(healthcheck.start_period, 30);
    }

    #[test]
    fn test_parse_healthcheck_minimal() {
        let kdl = r#"
            healthcheck {
                test "NONE"
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let healthcheck = parse_healthcheck(node.children().unwrap());

        assert_eq!(healthcheck.test, vec!["NONE".to_string()]);
        // デフォルト値が使われる
        assert_eq!(healthcheck.interval, 30);
        assert_eq!(healthcheck.timeout, 3);
        assert_eq!(healthcheck.retries, 3);
        assert_eq!(healthcheck.start_period, 10);
    }

    #[test]
    fn test_parse_wait_for_default() {
        let kdl = r#"
            service "api" {
                image "myapp:latest"
                depends_on "db"
                wait_for
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();

        assert!(service.wait_for.is_some());
        let wait_config = service.wait_for.unwrap();
        assert_eq!(wait_config.max_retries, 23); // デフォルト値
        assert_eq!(wait_config.initial_delay_ms, 1000);
        assert_eq!(wait_config.max_delay_ms, 30000);
        assert_eq!(wait_config.multiplier, 2.0);
    }

    #[test]
    fn test_parse_wait_for_custom() {
        let kdl = r#"
            service "api" {
                image "myapp:latest"
                depends_on "db"
                wait_for {
                    max_retries 10
                    initial_delay 500
                    max_delay 60000
                    multiplier 1.5
                }
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();

        assert!(service.wait_for.is_some());
        let wait_config = service.wait_for.unwrap();
        assert_eq!(wait_config.max_retries, 10);
        assert_eq!(wait_config.initial_delay_ms, 500);
        assert_eq!(wait_config.max_delay_ms, 60000);
        assert_eq!(wait_config.multiplier, 1.5);
    }

    #[test]
    fn test_parse_service_no_wait_for() {
        let kdl = r#"
            service "db" {
                image "postgres:16"
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();
        assert!(service.wait_for.is_none());
    }

    #[test]
    fn test_parse_readiness_property_style() {
        let kdl = r#"
            service "api" {
                image "myapp:latest"
                readiness path="/health" port=3000 timeout=60 interval=5
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();

        assert!(service.readiness.is_some());
        let readiness = service.readiness.unwrap();
        assert_eq!(readiness.path, "/health");
        assert_eq!(readiness.port, 3000);
        assert_eq!(readiness.timeout, 60);
        assert_eq!(readiness.interval, 5);
    }

    #[test]
    fn test_parse_readiness_block_style() {
        let kdl = r#"
            service "api" {
                image "myapp:latest"
                readiness {
                    path "/ready"
                    port 8080
                    timeout 45
                    interval 3
                }
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();

        assert!(service.readiness.is_some());
        let readiness = service.readiness.unwrap();
        assert_eq!(readiness.path, "/ready");
        assert_eq!(readiness.port, 8080);
        assert_eq!(readiness.timeout, 45);
        assert_eq!(readiness.interval, 3);
    }

    #[test]
    fn test_parse_readiness_defaults() {
        let kdl = r#"
            service "api" {
                image "myapp:latest"
                readiness port=3000
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();

        assert!(service.readiness.is_some());
        let readiness = service.readiness.unwrap();
        assert_eq!(readiness.path, "/health");
        assert_eq!(readiness.port, 3000);
        assert_eq!(readiness.timeout, 30);
        assert_eq!(readiness.interval, 2);
    }

    #[test]
    fn test_parse_service_no_readiness() {
        let kdl = r#"
            service "db" {
                image "postgres:16"
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();
        assert!(service.readiness.is_none());
    }

    #[test]
    fn test_parse_readiness_with_healthcheck() {
        let kdl = r#"
            service "api" {
                image "myapp:latest"
                healthcheck {
                    test "CMD-SHELL" "curl -f http://localhost:3000/health || exit 1"
                }
                readiness port=3000
            }
        "#;
        let doc: KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, service) = parse_service(node).unwrap();

        assert!(service.healthcheck.is_some());
        assert!(service.readiness.is_some());
        assert_eq!(service.readiness.unwrap().port, 3000);
    }

    #[test]
    fn test_wait_config_delay_calculation() {
        let config = WaitConfig {
            max_retries: 5,
            initial_delay_ms: 1000,
            max_delay_ms: 10000,
            multiplier: 2.0,
        };

        // 0回目: 1000ms
        assert_eq!(config.delay_for_attempt(0), 1000);
        // 1回目: 2000ms
        assert_eq!(config.delay_for_attempt(1), 2000);
        // 2回目: 4000ms
        assert_eq!(config.delay_for_attempt(2), 4000);
        // 3回目: 8000ms
        assert_eq!(config.delay_for_attempt(3), 8000);
        // 4回目: 16000ms -> max_delay(10000ms)でキャップ
        assert_eq!(config.delay_for_attempt(4), 10000);
    }
}
