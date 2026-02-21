use super::*;
use crate::model::{Port, Protocol, Volume};
use unison_kdl::{KdlDeserialize, KdlNodeExt, KdlSerialize};

#[test]
fn test_parse_simple_service() {
    let kdl = r#"
        service "postgres" {
            image "postgres:16"
            version "16"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    assert_eq!(flow.services.len(), 1);
    assert!(flow.services.contains_key("postgres"));

    let service = &flow.services["postgres"];
    assert_eq!(service.image, Some("postgres:16".to_string()));
    assert_eq!(service.version, Some("16".to_string()));
}

#[test]
#[ignore] // TODO: imageなしのエラー処理を後で精査
fn test_parse_service_without_image_error() {
    let kdl = r#"
        service "redis" {}
    "#;

    // imageなしはエラー
    let result = parse_kdl_string(kdl, "test".to_string());
    assert!(result.is_err());
}

#[test]
fn test_parse_service_with_explicit_image() {
    let kdl = r#"
        service "api" {
            image "myapp:1.0.0"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    // 明示的なimageが優先
    assert_eq!(service.image, Some("myapp:1.0.0".to_string()));
}

#[test]
fn test_parse_service_with_ports() {
    let kdl = r#"
        service "web" {
            image "nginx:latest"
            ports {
                port 8080 3000
                port 8443 3443 protocol="tcp"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["web"];

    assert_eq!(service.ports.len(), 2);

    let port1 = &service.ports[0];
    assert_eq!(port1.host, 8080);
    assert_eq!(port1.container, 3000);
    assert_eq!(port1.protocol, Protocol::Tcp);

    let port2 = &service.ports[1];
    assert_eq!(port2.host, 8443);
    assert_eq!(port2.container, 3443);
}

#[test]
fn test_parse_service_with_environment() {
    let kdl = r#"
        service "api" {
            image "node:20"
            environment {
                NODE_ENV "production"
                DATABASE_URL "postgresql://db:5432/mydb"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    assert_eq!(service.environment.len(), 2);
    assert_eq!(service.environment["NODE_ENV"], "production");
    assert_eq!(
        service.environment["DATABASE_URL"],
        "postgresql://db:5432/mydb"
    );
}

// Issue #12: env と environment 両方をサポート
#[test]
fn test_parse_service_with_env_alias() {
    let kdl = r#"
        service "api" {
            image "node:20"
            env {
                NODE_ENV "development"
                DEBUG "true"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    // env キーワードでも environment と同様に動作する
    assert_eq!(service.environment.len(), 2);
    assert_eq!(service.environment["NODE_ENV"], "development");
    assert_eq!(service.environment["DEBUG"], "true");
}

#[test]
fn test_parse_service_with_volumes() {
    let kdl = r#"
        service "db" {
            image "postgres:16"
            volumes {
                volume "./data" "/var/lib/postgresql/data"
                volume "./config" "/etc/config" read_only=#true
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["db"];

    assert_eq!(service.volumes.len(), 2);

    let vol1 = &service.volumes[0];
    assert_eq!(vol1.host.to_str().unwrap(), "./data");
    assert_eq!(vol1.container.to_str().unwrap(), "/var/lib/postgresql/data");
    assert!(!vol1.read_only);

    let vol2 = &service.volumes[1];
    assert!(vol2.read_only);
}

// Issue #13: 文字列 "true"/"false" でも動作する（警告は出る）
#[test]
fn test_parse_volume_with_string_bool() {
    let kdl = r#"
        service "db" {
            image "postgres:16"
            volumes {
                volume "./config" "/etc/config" read_only="true"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["db"];

    // 文字列 "true" でも警告付きで動作する
    assert_eq!(service.volumes.len(), 1);
    let vol = &service.volumes[0];
    assert!(vol.read_only);
}

#[test]
fn test_parse_service_with_depends_on() {
    let kdl = r#"
        service "api" {
            image "node:20"
            depends_on "db" "redis"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    assert_eq!(service.depends_on.len(), 2);
    assert!(service.depends_on.contains(&"db".to_string()));
    assert!(service.depends_on.contains(&"redis".to_string()));
}

#[test]
fn test_parse_stage() {
    let kdl = r#"
        service "api" { image "node:20" }
        service "db" { image "postgres:16" }
        service "redis" { image "redis:7" }

        stage "live" {
            service "api"
            service "db"
            service "redis"
            variables {
                DEBUG "false"
                LOG_LEVEL "info"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    assert_eq!(flow.stages.len(), 1);

    let stage = &flow.stages["live"];
    assert_eq!(stage.services.len(), 3);
    assert!(stage.services.contains(&"api".to_string()));

    assert_eq!(stage.variables.len(), 2);
    assert_eq!(stage.variables["DEBUG"], "false");
    assert_eq!(stage.variables["LOG_LEVEL"], "info");
}

#[test]
fn test_parse_multiple_services_and_stages() {
    let kdl = r#"
        service "api" {
            image "myapp:1.0.0"
            version "1.0.0"
        }

        service "db" {
            image "postgres:16"
            version "16"
        }

        stage "dev" {
            service "api"
            service "db"
        }

        stage "live" {
            service "api"
            service "db"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    assert_eq!(flow.services.len(), 2);
    assert_eq!(flow.stages.len(), 2);
}

#[test]
fn test_parse_port_with_host_ip() {
    let kdl = r#"
        service "web" {
            image "nginx:latest"
            ports {
                port 5432 5432 host_ip="127.0.0.1"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["web"];
    let port = &service.ports[0];

    assert_eq!(port.host_ip, Some("127.0.0.1".to_string()));
}

#[test]
fn test_parse_minimal_service() {
    let kdl = r#"
        service "minimal" {
            image "alpine:latest"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["minimal"];

    // デフォルト値の確認
    assert_eq!(service.image, Some("alpine:latest".to_string()));
    assert_eq!(service.ports.len(), 0);
    assert_eq!(service.environment.len(), 0);
    assert_eq!(service.volumes.len(), 0);
    assert_eq!(service.depends_on.len(), 0);
}

#[test]
fn test_invalid_service_no_name() {
    let kdl = r#"
        service {
            image "myapp:1.0"
        }
    "#;

    // サービス名がない → エラー
    assert!(parse_kdl_string(kdl, "test".to_string()).is_err());
}

#[test]
fn test_invalid_stage_no_name() {
    let kdl = r#"
        service "api" { image "node:20" }
        stage {
            service "api"
        }
    "#;

    // ステージ名がない → エラー
    assert!(parse_kdl_string(kdl, "test".to_string()).is_err());
}

#[test]
fn test_parse_service_with_command() {
    let kdl = r#"
        service "surrealdb" {
            image "surrealdb/surrealdb"
            version "latest"
            command "start --user root --pass root --bind [::]:8000 rocksdb://database.db"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["surrealdb"];

    // 明示的なimageが設定されている場合は、そのまま使われる
    assert_eq!(service.image, Some("surrealdb/surrealdb".to_string()));
    assert_eq!(service.version, Some("latest".to_string()));
    assert_eq!(
        service.command,
        Some("start --user root --pass root --bind [::]:8000 rocksdb://database.db".to_string())
    );
}

#[test]
fn test_parse_service_without_command() {
    let kdl = r#"
        service "postgres" {
            image "postgres:16"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["postgres"];

    assert_eq!(service.command, None);
}

#[test]
fn test_parse_project_name() {
    let kdl = r#"
        project "my-project"

        service "api" {
            image "myapp:1.0"
        }
    "#;

    let flow = parse_kdl_string(kdl, "default".to_string()).unwrap();

    // projectノードで指定した名前が使われる
    assert_eq!(flow.name, "my-project");
}

#[test]
fn test_parse_project_name_fallback() {
    let kdl = r#"
        service "api" {
            image "myapp:1.0"
        }
    "#;

    let flow = parse_kdl_string(kdl, "fallback-name".to_string()).unwrap();

    // projectノードがない場合はデフォルト名が使われる
    assert_eq!(flow.name, "fallback-name");
}

#[test]
fn test_parse_full_flow_with_project() {
    let kdl = r#"
        project "fleetflow"

        service "postgres" {
            image "postgres:16"
        }

        service "redis" {
            image "redis:7"
        }

        stage "local" {
            service "postgres"
            service "redis"
        }
    "#;

    let flow = parse_kdl_string(kdl, "default".to_string()).unwrap();

    assert_eq!(flow.name, "fleetflow");
    assert_eq!(flow.services.len(), 2);
    assert_eq!(flow.stages.len(), 1);
    assert_eq!(flow.stages["local"].services.len(), 2);
}

// Issue #15: クラウドリソースのパース
#[test]
fn test_parse_cloud_provider() {
    let kdl = r#"
        provider "sakura-cloud" {
            zone "tk1a"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    assert_eq!(flow.providers.len(), 1);

    let provider = &flow.providers["sakura-cloud"];
    assert_eq!(provider.zone, Some("tk1a".to_string()));
}

#[test]
fn test_parse_cloud_server() {
    let kdl = r#"
        server "creo-vps" {
            provider "sakura-cloud"
            plan "2core-4gb"
            disk_size 100
            os "ubuntu-24.04"
            ssh_keys "my-key"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    assert_eq!(flow.servers.len(), 1);

    let server = &flow.servers["creo-vps"];
    assert_eq!(server.provider, "sakura-cloud");
    assert_eq!(server.plan, Some("2core-4gb".to_string()));
    assert_eq!(server.disk_size, Some(100));
    assert_eq!(server.os, Some("ubuntu-24.04".to_string()));
    assert_eq!(server.ssh_keys.len(), 1);
}

#[test]
fn test_parse_stage_with_servers() {
    let kdl = r#"
        service "api" { image "node:20" }

        stage "live" {
            server "vps-01"
            service "api"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let stage = &flow.stages["live"];

    assert_eq!(stage.servers.len(), 1);
    assert!(stage.servers.contains(&"vps-01".to_string()));
    assert_eq!(stage.services.len(), 1);
}

#[test]
fn test_parse_full_cloud_config() {
    let kdl = r#"
        project "creo-memories"

        provider "sakura-cloud" {
            zone "tk1a"
        }

        server "creo-vps" {
            provider "sakura-cloud"
            plan "2core-4gb"
            disk_size 100
        }

        service "surrealdb" {
            image "surrealdb/surrealdb"
            version "latest"
        }

        stage "live" {
            server "creo-vps"
            service "surrealdb"
        }
    "#;

    let flow = parse_kdl_string(kdl, "default".to_string()).unwrap();

    assert_eq!(flow.name, "creo-memories");
    assert_eq!(flow.providers.len(), 1);
    assert_eq!(flow.servers.len(), 1);
    assert_eq!(flow.services.len(), 1);
    assert_eq!(flow.stages.len(), 1);

    let stage = &flow.stages["live"];
    assert_eq!(stage.servers.len(), 1);
    assert_eq!(stage.services.len(), 1);
}

// Issue: サービスマージロジックのテスト
// 複数のファイルで同じサービスを定義した場合、マージされることを確認

#[test]
fn test_service_merge_environment() {
    // fleet.kdl と flow.local.kdl を結合したような状態をシミュレート
    let kdl = r#"
        service "api" {
            image "myapp:latest"
            env {
                NODE_ENV "production"
                LOG_LEVEL "info"
                DATABASE_URL "postgresql://db:5432/mydb"
            }
        }

        service "api" {
            env {
                DATABASE_URL "postgresql://localhost:5432/mydb_dev"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    // image は保持される
    assert_eq!(service.image, Some("myapp:latest".to_string()));

    // environment はマージされる（後の定義が優先）
    assert_eq!(service.environment.len(), 3);
    assert_eq!(service.environment["NODE_ENV"], "production"); // 保持
    assert_eq!(service.environment["LOG_LEVEL"], "info"); // 保持
    assert_eq!(
        service.environment["DATABASE_URL"],
        "postgresql://localhost:5432/mydb_dev"
    ); // オーバーライド
}

#[test]
fn test_service_merge_ports_preserved() {
    let kdl = r#"
        service "api" {
            image "myapp:latest"
            ports {
                port 8080 3000
                port 8443 3443
            }
        }

        service "api" {
            env {
                DEBUG "true"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    // image は保持される
    assert_eq!(service.image, Some("myapp:latest".to_string()));

    // ports は空でなければ保持される（後の定義にportsがないので元が残る）
    assert_eq!(service.ports.len(), 2);
    assert_eq!(service.ports[0].host, 8080);
    assert_eq!(service.ports[1].host, 8443);

    // environment はマージされる
    assert_eq!(service.environment.len(), 1);
    assert_eq!(service.environment["DEBUG"], "true");
}

#[test]
fn test_service_merge_ports_overwritten() {
    let kdl = r#"
        service "api" {
            image "myapp:latest"
            ports {
                port 8080 3000
                port 8443 3443
            }
        }

        service "api" {
            ports {
                port 9000 3000
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    // image は保持される
    assert_eq!(service.image, Some("myapp:latest".to_string()));

    // ports は後の定義で上書き（空でないので）
    assert_eq!(service.ports.len(), 1);
    assert_eq!(service.ports[0].host, 9000);
}

#[test]
fn test_service_merge_version_override() {
    let kdl = r#"
        service "api" {
            image "myapp:latest"
            version "1.0.0"
            ports {
                port 8080 3000
            }
            env {
                NODE_ENV "production"
            }
        }

        service "api" {
            version "local"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    // version は上書き
    assert_eq!(service.version, Some("local".to_string()));

    // image は保持（version が変わっても image はそのまま）
    assert_eq!(service.image, Some("myapp:latest".to_string()));

    // ports, env も保持
    assert_eq!(service.ports.len(), 1);
    assert_eq!(service.environment.len(), 1);
}

#[test]
fn test_service_merge_volumes_preserved() {
    let kdl = r#"
        service "db" {
            image "postgres:16"
            volumes {
                volume "./data" "/var/lib/postgresql/data"
            }
        }

        service "db" {
            env {
                POSTGRES_PASSWORD "secret"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["db"];

    // volumes は保持される
    assert_eq!(service.volumes.len(), 1);
    assert_eq!(service.volumes[0].host.to_str().unwrap(), "./data");

    // environment はマージされる
    assert_eq!(service.environment.len(), 1);
    assert_eq!(service.environment["POSTGRES_PASSWORD"], "secret");
}

#[test]
fn test_service_merge_depends_on_preserved() {
    let kdl = r#"
        service "api" {
            image "myapp:latest"
            depends_on "db" "redis"
        }

        service "api" {
            env {
                API_KEY "test"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    // depends_on は保持される
    assert_eq!(service.depends_on.len(), 2);
    assert!(service.depends_on.contains(&"db".to_string()));
    assert!(service.depends_on.contains(&"redis".to_string()));

    // environment はマージされる
    assert_eq!(service.environment["API_KEY"], "test");
}

#[test]
fn test_service_merge_multiple_overrides() {
    // 3回のオーバーライドをシミュレート
    let kdl = r#"
        service "api" {
            image "myapp:latest"
            env {
                NODE_ENV "production"
                LOG_LEVEL "info"
            }
        }

        service "api" {
            env {
                LOG_LEVEL "debug"
                DEBUG "true"
            }
        }

        service "api" {
            env {
                API_KEY "secret"
            }
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];

    // 全ての環境変数がマージされる
    assert_eq!(service.environment.len(), 4);
    assert_eq!(service.environment["NODE_ENV"], "production"); // 1回目から
    assert_eq!(service.environment["LOG_LEVEL"], "debug"); // 2回目で上書き
    assert_eq!(service.environment["DEBUG"], "true"); // 2回目で追加
    assert_eq!(service.environment["API_KEY"], "secret"); // 3回目で追加
}

// ============================================================================
// unison-kdl 直接パーステスト
// ============================================================================

#[test]
fn test_port_unison_kdl_deserialize() {
    let kdl = r#"port host=8080 container=3000"#;
    let doc: kdl::KdlDocument = kdl.parse().unwrap();
    let node = doc.nodes().first().unwrap();

    let port = Port::from_kdl_node(node).unwrap();
    assert_eq!(port.host, 8080);
    assert_eq!(port.container, 3000);
    assert_eq!(port.protocol, Protocol::Tcp); // default
    assert_eq!(port.host_ip, None);
}

#[test]
fn test_port_unison_kdl_serialize() {
    let port = Port {
        host: 8080,
        container: 3000,
        protocol: Protocol::Tcp,
        host_ip: None,
    };

    let node = port.to_kdl_node().unwrap();
    assert_eq!(node.name().value(), "port");

    // プロパティの確認
    assert_eq!(node.prop("host").and_then(|v| v.as_integer()), Some(8080));
    assert_eq!(
        node.prop("container").and_then(|v| v.as_integer()),
        Some(3000)
    );
}

#[test]
fn test_port_unison_kdl_roundtrip() {
    let original = Port {
        host: 9090,
        container: 80,
        protocol: Protocol::Udp,
        host_ip: Some("127.0.0.1".to_string()),
    };

    // Serialize -> Deserialize
    let node = original.to_kdl_node().unwrap();
    let deserialized = Port::from_kdl_node(&node).unwrap();

    assert_eq!(deserialized.host, original.host);
    assert_eq!(deserialized.container, original.container);
    assert_eq!(deserialized.protocol, original.protocol);
    assert_eq!(deserialized.host_ip, original.host_ip);
}

#[test]
fn test_volume_unison_kdl_deserialize() {
    let kdl = r#"volume "./data" "/var/lib/data""#;
    let doc: kdl::KdlDocument = kdl.parse().unwrap();
    let node = doc.nodes().first().unwrap();

    let volume = Volume::from_kdl_node(node).unwrap();
    assert_eq!(volume.host.to_str().unwrap(), "./data");
    assert_eq!(volume.container.to_str().unwrap(), "/var/lib/data");
    assert!(!volume.read_only); // default
}

#[test]
fn test_volume_unison_kdl_with_readonly() {
    let kdl = r#"volume "./config" "/etc/config" read_only=#true"#;
    let doc: kdl::KdlDocument = kdl.parse().unwrap();
    let node = doc.nodes().first().unwrap();

    let volume = Volume::from_kdl_node(node).unwrap();
    assert_eq!(volume.host.to_str().unwrap(), "./config");
    assert_eq!(volume.container.to_str().unwrap(), "/etc/config");
    assert!(volume.read_only);
}

#[test]
fn test_volume_unison_kdl_serialize() {
    let volume = Volume {
        host: std::path::PathBuf::from("./data"),
        container: std::path::PathBuf::from("/var/lib/data"),
        read_only: false,
    };

    let node = volume.to_kdl_node().unwrap();
    assert_eq!(node.name().value(), "volume");

    // 引数の確認（位置引数）
    assert_eq!(node.arg(0).and_then(|v| v.as_string()), Some("./data"));
    assert_eq!(
        node.arg(1).and_then(|v| v.as_string()),
        Some("/var/lib/data")
    );
}

#[test]
fn test_volume_unison_kdl_roundtrip() {
    let original = Volume {
        host: std::path::PathBuf::from("/host/path"),
        container: std::path::PathBuf::from("/container/path"),
        read_only: true,
    };

    // Serialize -> Deserialize
    let node = original.to_kdl_node().unwrap();
    let deserialized = Volume::from_kdl_node(&node).unwrap();

    assert_eq!(deserialized.host, original.host);
    assert_eq!(deserialized.container, original.container);
    assert_eq!(deserialized.read_only, original.read_only);
}

// ============================================================================
// 変数展開テスト
// ============================================================================

#[test]
fn test_parse_with_variables() {
    let kdl = r#"
        variables {
            registry "ghcr.io/myorg"
            version "1.0.0"
        }

        service "api" {
            image "{{ registry }}/api:{{ version }}"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];
    assert_eq!(service.image, Some("ghcr.io/myorg/api:1.0.0".to_string()));
}

#[test]
fn test_parse_with_multiple_variables() {
    let kdl = r#"
        variables {
            registry "ghcr.io/myorg"
            api_version "2.0.0"
            worker_version "1.5.0"
        }

        service "api" {
            image "{{ registry }}/api:{{ api_version }}"
        }

        service "worker" {
            image "{{ registry }}/worker:{{ worker_version }}"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    assert_eq!(flow.services.len(), 2);

    let api = &flow.services["api"];
    assert_eq!(api.image, Some("ghcr.io/myorg/api:2.0.0".to_string()));

    let worker = &flow.services["worker"];
    assert_eq!(worker.image, Some("ghcr.io/myorg/worker:1.5.0".to_string()));
}

#[test]
fn test_parse_without_variables() {
    let kdl = r#"
        service "api" {
            image "myapp:1.0.0"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["api"];
    assert_eq!(service.image, Some("myapp:1.0.0".to_string()));
}

// ============================================================================
// include テスト
// ============================================================================

#[test]
fn test_include_single_file() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let base_path = temp_dir.path();

    std::fs::write(
        base_path.join("redis.kdl"),
        r#"service "redis" { version "7" }"#,
    )
    .unwrap();

    let main_kdl = r#"
include "redis.kdl"

service "postgres" {
    version "16"
}
"#;
    std::fs::write(base_path.join("main.kdl"), main_kdl).unwrap();

    let flow = parse_kdl_file(base_path.join("main.kdl")).unwrap();

    assert_eq!(flow.services.len(), 2);
    assert!(flow.services.contains_key("redis"));
    assert!(flow.services.contains_key("postgres"));
    assert_eq!(flow.services["redis"].version, Some("7".to_string()));
    assert_eq!(flow.services["postgres"].version, Some("16".to_string()));
}

#[test]
fn test_include_glob_pattern() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let base_path = temp_dir.path();

    std::fs::create_dir(base_path.join("services")).unwrap();

    std::fs::write(
        base_path.join("services/redis.kdl"),
        r#"service "redis" { version "7" }"#,
    )
    .unwrap();

    std::fs::write(
        base_path.join("services/postgres.kdl"),
        r#"service "postgres" { version "16" }"#,
    )
    .unwrap();

    let main_kdl = r#"
include "services/*.kdl"

service "api" {
    version "1.0"
}
"#;
    std::fs::write(base_path.join("main.kdl"), main_kdl).unwrap();

    let flow = parse_kdl_file(base_path.join("main.kdl")).unwrap();

    assert_eq!(flow.services.len(), 3);
    assert!(flow.services.contains_key("redis"));
    assert!(flow.services.contains_key("postgres"));
    assert!(flow.services.contains_key("api"));
}

#[test]
fn test_include_circular_reference() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let base_path = temp_dir.path();

    std::fs::write(base_path.join("a.kdl"), r#"include "b.kdl""#).unwrap();
    std::fs::write(base_path.join("b.kdl"), r#"include "a.kdl""#).unwrap();

    let result = parse_kdl_file(base_path.join("a.kdl"));
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Circular include"),
        "エラーメッセージに循環参照が示されるべき: {}",
        err
    );
}

#[test]
fn test_include_nested() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let base_path = temp_dir.path();

    std::fs::write(
        base_path.join("level2.kdl"),
        r#"service "redis" { version "7" }"#,
    )
    .unwrap();

    std::fs::write(
        base_path.join("level1.kdl"),
        "include \"level2.kdl\"\nservice \"postgres\" { version \"16\" }",
    )
    .unwrap();

    std::fs::write(
        base_path.join("main.kdl"),
        "include \"level1.kdl\"\nservice \"api\" { version \"1.0\" }",
    )
    .unwrap();

    let flow = parse_kdl_file(base_path.join("main.kdl")).unwrap();

    assert_eq!(flow.services.len(), 3);
    assert!(flow.services.contains_key("redis"));
    assert!(flow.services.contains_key("postgres"));
    assert!(flow.services.contains_key("api"));
}
