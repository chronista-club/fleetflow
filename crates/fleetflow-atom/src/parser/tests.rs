use super::*;
use crate::model::Protocol;

#[test]
fn test_parse_simple_service() {
    let kdl = r#"
        service "postgres" {
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
fn test_parse_service_without_version() {
    let kdl = r#"
        service "redis" {}
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["redis"];

    // バージョン未指定 → image は redis:latest
    assert_eq!(service.image, Some("redis:latest".to_string()));
    assert_eq!(service.version, None);
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

#[test]
fn test_parse_service_with_volumes() {
    let kdl = r#"
        service "db" {
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
    assert_eq!(vol1.read_only, false);

    let vol2 = &service.volumes[1];
    assert_eq!(vol2.read_only, true);
}

#[test]
fn test_parse_service_with_depends_on() {
    let kdl = r#"
        service "api" {
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
        stage "production" {
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

    let stage = &flow.stages["production"];
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
            version "1.0.0"
        }

        service "db" {
            version "16"
        }

        stage "dev" {
            service "api"
            service "db"
        }

        stage "prod" {
            service "api"
            service "db"
        }
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    assert_eq!(flow.services.len(), 2);
    assert_eq!(flow.stages.len(), 2);
}

#[test]
fn test_infer_image_name() {
    assert_eq!(infer_image_name("postgres", None), "postgres:latest");
    assert_eq!(infer_image_name("postgres", Some("16")), "postgres:16");
    assert_eq!(
        infer_image_name("node", Some("20-alpine")),
        "node:20-alpine"
    );
    assert_eq!(infer_image_name("redis", Some("7")), "redis:7");
}

#[test]
fn test_parse_port_with_host_ip() {
    let kdl = r#"
        service "web" {
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
fn test_parse_empty_service() {
    let kdl = r#"
        service "minimal" {}
    "#;

    let flow = parse_kdl_string(kdl, "test".to_string()).unwrap();
    let service = &flow.services["minimal"];

    // デフォルト値の確認
    assert_eq!(service.image, Some("minimal:latest".to_string()));
    assert_eq!(service.ports.len(), 0);
    assert_eq!(service.environment.len(), 0);
    assert_eq!(service.volumes.len(), 0);
    assert_eq!(service.depends_on.len(), 0);
}

#[test]
fn test_invalid_service_no_name() {
    let kdl = r#"
        service {
            version "1.0"
        }
    "#;

    // サービス名がない → エラー
    assert!(parse_kdl_string(kdl, "test".to_string()).is_err());
}

#[test]
fn test_invalid_stage_no_name() {
    let kdl = r#"
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
            version "16"
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
            version "1.0"
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
            version "1.0"
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
            version "16"
        }

        service "redis" {
            version "7"
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
