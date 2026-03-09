use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use fleetflow_controlplane::server::ServerConfig;

/// fleetflowd の設定（fleetflowd.kdl から読み込み）
#[derive(Debug)]
pub struct DaemonConfig {
    pub pid_file: PathBuf,
    pub log_file: Option<PathBuf>,
    pub log_level: String,
    pub server: ServerConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("fleetflow");

        Self {
            pid_file: data_dir.join("fleetflowd.pid"),
            log_file: None,
            log_level: "info".into(),
            server: ServerConfig::default(),
        }
    }
}

/// 設定ファイルの探索順:
/// 1. --config で指定されたパス
/// 2. ./fleetflowd.kdl
/// 3. ~/.config/fleetflow/fleetflowd.kdl
/// 4. /etc/fleetflow/fleetflowd.kdl
pub fn find_config_file(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        if path.exists() {
            return Some(path.to_path_buf());
        }
    }

    let candidates = [
        Some(PathBuf::from("fleetflowd.kdl")),
        dirs::config_dir().map(|d| d.join("fleetflow/fleetflowd.kdl")),
        Some(PathBuf::from("/etc/fleetflow/fleetflowd.kdl")),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists())
}

/// KDL ノードから最初の文字列引数を取得するヘルパー
fn node_str<'a>(node: &'a kdl::KdlNode) -> Option<&'a str> {
    node.entries().first().and_then(|e| e.value().as_string())
}

/// KDL ファイルから設定を読み込み
pub fn load_config(path: &Path) -> Result<DaemonConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("設定ファイル読み込み失敗: {}", path.display()))?;

    let doc: kdl::KdlDocument = content
        .parse()
        .with_context(|| format!("KDL パース失敗: {}", path.display()))?;

    let mut config = DaemonConfig::default();

    // daemon ノード
    if let Some(daemon) = doc.get("daemon") {
        if let Some(children) = daemon.children() {
            if let Some(node) = children.get("pid-file") {
                if let Some(val) = node_str(node) {
                    config.pid_file = PathBuf::from(resolve_env(val));
                }
            }
            if let Some(node) = children.get("log-file") {
                if let Some(val) = node_str(node) {
                    config.log_file = Some(PathBuf::from(resolve_env(val)));
                }
            }
            if let Some(node) = children.get("log-level") {
                if let Some(val) = node_str(node) {
                    config.log_level = val.to_string();
                }
            }
        }
    }

    // api ノード
    if let Some(api) = doc.get("api") {
        if let Some(children) = api.children() {
            if let Some(node) = children.get("listen") {
                if let Some(val) = node_str(node) {
                    config.server.listen_addr = val.to_string();
                }
            }
        }
    }

    // database ノード
    if let Some(database) = doc.get("database") {
        if let Some(children) = database.children() {
            let db = &mut config.server.db;
            if let Some(node) = children.get("endpoint") {
                if let Some(val) = node_str(node) {
                    db.endpoint = resolve_env(val);
                }
            }
            if let Some(node) = children.get("namespace") {
                if let Some(val) = node_str(node) {
                    db.namespace = val.to_string();
                }
            }
            if let Some(node) = children.get("database") {
                if let Some(val) = node_str(node) {
                    db.database = val.to_string();
                }
            }
            if let Some(node) = children.get("username") {
                if let Some(val) = node_str(node) {
                    db.username = val.to_string();
                }
            }
            if let Some(node) = children.get("password") {
                if let Some(val) = node_str(node) {
                    db.password = resolve_env(val);
                }
            }
        }
    }

    // auth ノード
    if let Some(auth) = doc.get("auth") {
        if let Some(children) = auth.children() {
            let a = &mut config.server.auth;
            if let Some(node) = children.get("domain") {
                if let Some(val) = node_str(node) {
                    a.domain = val.to_string();
                }
            }
            if let Some(node) = children.get("audience") {
                if let Some(val) = node_str(node) {
                    a.audience = val.to_string();
                }
            }
        }
    }

    Ok(config)
}

/// 環境変数の展開: `${VAR_NAME}` → 環境変数の値
fn resolve_env(val: &str) -> String {
    if val.starts_with("${") && val.ends_with('}') {
        let var_name = &val[2..val.len() - 1];
        std::env::var(var_name).unwrap_or_default()
    } else {
        val.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_env() {
        assert_eq!(resolve_env("hello"), "hello");
        unsafe { std::env::set_var("TEST_FLEET_VAR", "secret123") };
        assert_eq!(resolve_env("${TEST_FLEET_VAR}"), "secret123");
        unsafe { std::env::remove_var("TEST_FLEET_VAR") };
    }

    #[test]
    fn test_default_config() {
        let config = DaemonConfig::default();
        assert_eq!(config.log_level, "info");
        assert_eq!(config.server.listen_addr, "[::1]:4510");
    }

    #[test]
    fn test_load_config_from_kdl() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("fleetflowd.kdl");
        std::fs::write(
            &config_path,
            r#"
daemon {
    pid-file "/tmp/test-fleetflowd.pid"
    log-level "debug"
}

api {
    listen "0.0.0.0:5510"
}

database {
    endpoint "ws://db.example.com:12000"
    namespace "myns"
    database "mydb"
}

auth {
    domain "example.auth0.com"
    audience "https://api.example.com"
}
"#,
        )
        .unwrap();

        let config = load_config(&config_path).unwrap();
        assert_eq!(config.pid_file, PathBuf::from("/tmp/test-fleetflowd.pid"));
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.server.listen_addr, "0.0.0.0:5510");
        assert_eq!(config.server.db.endpoint, "ws://db.example.com:12000");
        assert_eq!(config.server.db.namespace, "myns");
        assert_eq!(config.server.auth.domain, "example.auth0.com");
    }
}
