//! Fleet Registry データモデル

use fleetflow_core::ServerResource;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Fleet Registry — 複数fleetとサーバーの統合管理
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    /// Registry名（リポジトリ識別子）
    pub name: String,

    /// Fleet定義のマップ（fleet名 → FleetEntry）
    pub fleets: HashMap<String, FleetEntry>,

    /// サーバー定義のマップ（server名 → ServerResource）
    pub servers: HashMap<String, ServerResource>,

    /// デプロイルーティング
    pub routes: Vec<DeploymentRoute>,
}

/// 個別FleetFlowプロジェクトへの参照
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FleetEntry {
    /// Registryルートからの相対パス
    pub path: PathBuf,

    /// 説明（オプション）
    pub description: Option<String>,
}

/// デプロイルーティング: fleet+stage → server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRoute {
    /// 対象fleet名
    pub fleet: String,

    /// 対象stage名
    pub stage: String,

    /// デプロイ先server名
    pub server: String,
}

impl Registry {
    /// 指定したfleet+stageのデプロイルートを解決する
    pub fn resolve_route(&self, fleet: &str, stage: &str) -> Option<&DeploymentRoute> {
        self.routes
            .iter()
            .find(|r| r.fleet == fleet && r.stage == stage)
    }

    /// 指定したサーバーにルーティングされている全ルートを取得
    pub fn routes_for_server(&self, server: &str) -> Vec<&DeploymentRoute> {
        self.routes.iter().filter(|r| r.server == server).collect()
    }

    /// 指定したfleetの全ルートを取得
    pub fn routes_for_fleet(&self, fleet: &str) -> Vec<&DeploymentRoute> {
        self.routes.iter().filter(|r| r.fleet == fleet).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registry() -> Registry {
        let mut fleets = HashMap::new();
        fleets.insert(
            "creo".to_string(),
            FleetEntry {
                path: PathBuf::from("fleets/creo"),
                description: Some("Creo Memories".to_string()),
            },
        );
        fleets.insert(
            "bikeboy".to_string(),
            FleetEntry {
                path: PathBuf::from("fleets/bikeboy"),
                description: Some("BikeBoy".to_string()),
            },
        );

        let mut servers = HashMap::new();
        servers.insert(
            "vps-01".to_string(),
            ServerResource {
                provider: "sakura-cloud".to_string(),
                plan: Some("4core-8gb".to_string()),
                ..Default::default()
            },
        );

        let routes = vec![
            DeploymentRoute {
                fleet: "creo".to_string(),
                stage: "live".to_string(),
                server: "vps-01".to_string(),
            },
            DeploymentRoute {
                fleet: "bikeboy".to_string(),
                stage: "live".to_string(),
                server: "vps-01".to_string(),
            },
        ];

        Registry {
            name: "chronista-fleet".to_string(),
            fleets,
            servers,
            routes,
        }
    }

    #[test]
    fn test_resolve_route() {
        let registry = sample_registry();

        let route = registry.resolve_route("creo", "live");
        assert!(route.is_some());
        assert_eq!(route.unwrap().server, "vps-01");

        assert!(registry.resolve_route("creo", "dev").is_none());
        assert!(registry.resolve_route("unknown", "live").is_none());
    }

    #[test]
    fn test_routes_for_server() {
        let registry = sample_registry();
        let routes = registry.routes_for_server("vps-01");
        assert_eq!(routes.len(), 2);
    }

    #[test]
    fn test_routes_for_fleet() {
        let registry = sample_registry();
        let routes = registry.routes_for_fleet("creo");
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].stage, "live");
    }
}
