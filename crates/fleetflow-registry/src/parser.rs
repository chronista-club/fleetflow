//! fleet-registry.kdl パーサー
//!
//! Registry KDL構文をパースして Registry 構造体を生成する。
//! server ノードのパースは fleetflow-core の parse_server を再利用。

use crate::error::{RegistryError, Result};
use crate::model::{DeploymentRoute, FleetEntry, Registry};
use kdl::KdlDocument;
use std::path::{Path, PathBuf};

/// KDLファイルを Registry にパース
pub fn parse_registry_file(path: &Path) -> Result<Registry> {
    let content = std::fs::read_to_string(path)?;
    parse_registry(&content)
}

/// KDL文字列を Registry にパース
pub fn parse_registry(content: &str) -> Result<Registry> {
    let doc: KdlDocument = content.parse()?;

    let mut registry = Registry::default();

    for node in doc.nodes() {
        match node.name().value() {
            "registry" => {
                if let Some(name) = node.entries().first().and_then(|e| e.value().as_string()) {
                    registry.name = name.to_string();
                }
            }
            "fleet" => {
                let (name, entry) = parse_fleet_entry(node)?;
                registry.fleets.insert(name, entry);
            }
            "server" => {
                // fleetflow-core の parse_server を再利用
                let (name, server) = fleetflow_core::parse_server(node)
                    .map_err(|e| RegistryError::InvalidConfig(e.to_string()))?;
                registry.servers.insert(name, server);
            }
            "deployment" => {
                if let Some(children) = node.children() {
                    for child in children.nodes() {
                        if child.name().value() == "route" {
                            let route = parse_route(child)?;
                            registry.routes.push(route);
                        }
                    }
                }
            }
            _ => {
                // 不明なノードはスキップ
            }
        }
    }

    if registry.name.is_empty() {
        return Err(RegistryError::InvalidConfig(
            "registry ノードが必要です".to_string(),
        ));
    }

    // バリデーション: ルートが参照するfleetとserverが存在するか
    for route in &registry.routes {
        if !registry.fleets.contains_key(&route.fleet) {
            return Err(RegistryError::FleetNotFound(route.fleet.clone()));
        }
        if !registry.servers.contains_key(&route.server) {
            return Err(RegistryError::ServerNotFound(route.server.clone()));
        }
    }

    Ok(registry)
}

/// fleet ノードをパース
fn parse_fleet_entry(node: &kdl::KdlNode) -> Result<(String, FleetEntry)> {
    let name = node
        .entries()
        .first()
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| RegistryError::InvalidConfig("fleet には名前が必要です".to_string()))?
        .to_string();

    let mut entry = FleetEntry::default();

    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "path" => {
                    if let Some(path) = child.entries().first().and_then(|e| e.value().as_string())
                    {
                        entry.path = PathBuf::from(path);
                    }
                }
                "description" => {
                    entry.description = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                _ => {}
            }
        }
    }

    Ok((name, entry))
}

/// route ノードをパース
fn parse_route(node: &kdl::KdlNode) -> Result<DeploymentRoute> {
    let mut fleet = None;
    let mut stage = None;
    let mut server = None;

    for entry in node.entries() {
        if let Some(name) = entry.name() {
            match name.value() {
                "fleet" => fleet = entry.value().as_string().map(|s| s.to_string()),
                "stage" => stage = entry.value().as_string().map(|s| s.to_string()),
                "server" => server = entry.value().as_string().map(|s| s.to_string()),
                _ => {}
            }
        }
    }

    let fleet = fleet
        .ok_or_else(|| RegistryError::InvalidConfig("route に fleet が必要です".to_string()))?;
    let stage = stage
        .ok_or_else(|| RegistryError::InvalidConfig("route に stage が必要です".to_string()))?;
    let server = server
        .ok_or_else(|| RegistryError::InvalidConfig("route に server が必要です".to_string()))?;

    Ok(DeploymentRoute {
        fleet,
        stage,
        server,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_REGISTRY: &str = r#"
registry "chronista-fleet"

fleet "creo" {
    path "fleets/creo"
    description "Creo Memories - 永続記憶サービス"
}

fleet "bikeboy" {
    path "fleets/bikeboy"
    description "BikeBoy - 調査エージェント"
}

server "vps-01" {
    provider "sakura-cloud"
    plan "4core-8gb"
    ssh-key "deployment"
    deploy-path "/opt/apps"
}

deployment {
    route fleet="creo" stage="live" server="vps-01"
    route fleet="bikeboy" stage="live" server="vps-01"
}
"#;

    #[test]
    fn test_parse_registry_full() {
        let registry = parse_registry(SAMPLE_REGISTRY).unwrap();

        assert_eq!(registry.name, "chronista-fleet");
        assert_eq!(registry.fleets.len(), 2);
        assert_eq!(registry.servers.len(), 1);
        assert_eq!(registry.routes.len(), 2);

        // Fleet entries
        let creo = registry.fleets.get("creo").unwrap();
        assert_eq!(creo.path, PathBuf::from("fleets/creo"));
        assert_eq!(
            creo.description.as_deref(),
            Some("Creo Memories - 永続記憶サービス")
        );

        let bikeboy = registry.fleets.get("bikeboy").unwrap();
        assert_eq!(bikeboy.path, PathBuf::from("fleets/bikeboy"));

        // Server
        let vps = registry.servers.get("vps-01").unwrap();
        assert_eq!(vps.provider, "sakura-cloud");
        assert_eq!(vps.plan.as_deref(), Some("4core-8gb"));
        assert_eq!(vps.deploy_path.as_deref(), Some("/opt/apps"));

        // Routes
        let creo_route = registry.resolve_route("creo", "live").unwrap();
        assert_eq!(creo_route.server, "vps-01");
    }

    #[test]
    fn test_parse_registry_minimal() {
        let kdl = r#"
registry "test"

fleet "app" {
    path "fleets/app"
}

server "s1" {
    provider "sakura-cloud"
}

deployment {
    route fleet="app" stage="live" server="s1"
}
"#;
        let registry = parse_registry(kdl).unwrap();
        assert_eq!(registry.name, "test");
        assert_eq!(registry.fleets.len(), 1);
        assert_eq!(registry.servers.len(), 1);
        assert_eq!(registry.routes.len(), 1);
    }

    #[test]
    fn test_parse_registry_missing_name() {
        let kdl = r#"
fleet "app" {
    path "fleets/app"
}
"#;
        let err = parse_registry(kdl).unwrap_err();
        assert!(matches!(err, RegistryError::InvalidConfig(_)));
    }

    #[test]
    fn test_parse_registry_route_references_unknown_fleet() {
        let kdl = r#"
registry "test"

server "s1" {
    provider "sakura-cloud"
}

deployment {
    route fleet="unknown" stage="live" server="s1"
}
"#;
        let err = parse_registry(kdl).unwrap_err();
        assert!(matches!(err, RegistryError::FleetNotFound(_)));
    }

    #[test]
    fn test_parse_registry_route_references_unknown_server() {
        let kdl = r#"
registry "test"

fleet "app" {
    path "fleets/app"
}

deployment {
    route fleet="app" stage="live" server="unknown"
}
"#;
        let err = parse_registry(kdl).unwrap_err();
        assert!(matches!(err, RegistryError::ServerNotFound(_)));
    }

    #[test]
    fn test_parse_registry_with_ssh_info() {
        let kdl = r#"
registry "test-fleet"

fleet "creo" {
    path "fleets/creo"
    description "Creo Memories"
}

server "vps-01" {
    provider "sakura-cloud"
    plan "4core-8gb"
    ssh-key "deployment"
    ssh-host "153.120.168.42"
    ssh-user "root"
    deploy-path "/opt/apps"
}

deployment {
    route fleet="creo" stage="live" server="vps-01"
}
"#;
        let registry = parse_registry(kdl).unwrap();
        let vps = registry.servers.get("vps-01").unwrap();
        assert_eq!(vps.ssh_host.as_deref(), Some("153.120.168.42"));
        assert_eq!(vps.ssh_user.as_deref(), Some("root"));
        assert_eq!(vps.deploy_path.as_deref(), Some("/opt/apps"));
    }

    #[test]
    fn test_parse_registry_no_routes() {
        let kdl = r#"
registry "test"

fleet "app" {
    path "fleets/app"
}

server "s1" {
    provider "sakura-cloud"
}
"#;
        let registry = parse_registry(kdl).unwrap();
        assert!(registry.routes.is_empty());
    }
}
