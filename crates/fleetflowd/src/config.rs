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
    pub web_addr: String,
    /// Dashboard SPA 用 Auth0 Client ID
    pub auth0_client_id: String,
    /// ヘルスチェック間隔（秒）。0 で無効化。
    pub health_check_interval: u64,
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
            web_addr: "127.0.0.1:32080".into(),
            auth0_client_id: String::new(),
            health_check_interval: 60,
        }
    }
}

/// 設定ファイルの探索順:
/// 1. --config で指定されたパス
/// 2. ./fleetflowd.kdl
/// 3. ~/.config/fleetflow/fleetflowd.kdl
/// 4. /etc/fleetflow/fleetflowd.kdl
pub fn find_config_file(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit
        && path.exists()
    {
        return Some(path.to_path_buf());
    }

    let candidates = [
        Some(PathBuf::from("fleetflowd.kdl")),
        dirs::config_dir().map(|d| d.join("fleetflow/fleetflowd.kdl")),
        Some(PathBuf::from("/etc/fleetflow/fleetflowd.kdl")),
    ];

    candidates.into_iter().flatten().find(|p| p.exists())
}

/// KDL ノードから最初の文字列引数を取得するヘルパー
fn node_str(node: &kdl::KdlNode) -> Option<&str> {
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
    if let Some(daemon) = doc.get("daemon")
        && let Some(children) = daemon.children()
    {
        if let Some(node) = children.get("pid-file")
            && let Some(val) = node_str(node)
        {
            config.pid_file = PathBuf::from(resolve_env(val));
        }
        if let Some(node) = children.get("log-file")
            && let Some(val) = node_str(node)
        {
            config.log_file = Some(PathBuf::from(resolve_env(val)));
        }
        if let Some(node) = children.get("log-level")
            && let Some(val) = node_str(node)
        {
            config.log_level = val.to_string();
        }
    }

    // api ノード
    if let Some(api) = doc.get("api")
        && let Some(children) = api.children()
        && let Some(node) = children.get("listen")
        && let Some(val) = node_str(node)
    {
        config.server.listen_addr = val.to_string();
    }

    // database ノード
    if let Some(database) = doc.get("database")
        && let Some(children) = database.children()
    {
        let db = &mut config.server.db;
        if let Some(node) = children.get("endpoint")
            && let Some(val) = node_str(node)
        {
            db.endpoint = resolve_env(val);
        }
        if let Some(node) = children.get("namespace")
            && let Some(val) = node_str(node)
        {
            db.namespace = val.to_string();
        }
        if let Some(node) = children.get("database")
            && let Some(val) = node_str(node)
        {
            db.database = val.to_string();
        }
        if let Some(node) = children.get("username")
            && let Some(val) = node_str(node)
        {
            db.username = val.to_string();
        }
        if let Some(node) = children.get("password")
            && let Some(val) = node_str(node)
        {
            db.password = resolve_env(val);
        }
    }

    // web ノード
    if let Some(web) = doc.get("web")
        && let Some(children) = web.children()
    {
        if let Some(node) = children.get("listen")
            && let Some(val) = node_str(node)
        {
            config.web_addr = val.to_string();
        }
        if let Some(node) = children.get("auth0-client-id")
            && let Some(val) = node_str(node)
        {
            config.auth0_client_id = resolve_env(val);
        }
    }

    // auth ノード
    if let Some(auth) = doc.get("auth")
        && let Some(children) = auth.children()
    {
        let a = &mut config.server.auth;
        if let Some(node) = children.get("domain")
            && let Some(val) = node_str(node)
        {
            a.domain = val.to_string();
        }
        if let Some(node) = children.get("audience")
            && let Some(val) = node_str(node)
        {
            a.audience = val.to_string();
        }
    }

    // cloud ノード（サーバープロバイダー設定）
    if let Some(cloud) = doc.get("cloud")
        && let Some(children) = cloud.children()
        && let Some(node) = children.get("provider")
        && let Some(val) = node_str(node)
    {
        match val {
            "sakura-cloud" => {
                let zone = children
                    .get("zone")
                    .and_then(|n| node_str(n))
                    .unwrap_or("tk1a");
                let provider = fleetflow_cloud_sakura::SakuraCloudProvider::new(zone);
                config.server.server_provider = Some(
                    fleetflow_controlplane::server_provider::ServerProviderKind::Sakura(provider),
                );
            }
            "aws" => {
                let region = children
                    .get("region")
                    .and_then(|n| node_str(n))
                    .unwrap_or("ap-northeast-1");
                // AWS プロバイダーは async 初期化が必要なため、ここでは region だけ保持して
                // 起動時に初期化する方式にする（config パース時点では tokio ランタイム未確定）
                tracing::info!(region = region, "AWS provider configured (lazy init)");
                // TODO: 起動時の lazy init 実装。暫定で warn を出す。
                tracing::warn!("AWS provider: lazy init not yet implemented — use CP API to configure");
            }
            other => {
                tracing::warn!(
                    provider = other,
                    "未対応のクラウドプロバイダー（server CRUD は無効）"
                );
            }
        }
    }

    // health ノード
    if let Some(health) = doc.get("health")
        && let Some(children) = health.children()
        && let Some(node) = children.get("check-interval")
        && let Some(entry) = node.entries().first()
        && let Some(val) = entry.value().as_integer()
    {
        if val < 0 {
            anyhow::bail!("health check-interval は 0 以上を指定してください（現在: {val}）");
        }
        config.health_check_interval = val as u64;
    }

    Ok(config)
}

/// 環境変数の展開: 値全体が `${VAR_NAME}` の場合のみ展開する。
/// 部分的な展開（`prefix-${VAR}`）はサポートしない。
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
        assert_eq!(config.web_addr, "127.0.0.1:32080");
    }

    #[test]
    fn test_load_config_from_kdl() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("fleetflowd.kdl");
        unsafe { std::env::set_var("TEST_AUTH0_CLIENT", "test-client-id-123") };
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

web {
    listen "0.0.0.0:8080"
    auth0-client-id "${TEST_AUTH0_CLIENT}"
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
        assert_eq!(config.web_addr, "0.0.0.0:8080");
        assert_eq!(config.auth0_client_id, "test-client-id-123");
        unsafe { std::env::remove_var("TEST_AUTH0_CLIENT") };
    }
}
