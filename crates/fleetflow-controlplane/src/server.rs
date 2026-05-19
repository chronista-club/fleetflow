use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;
use unison::network::server::{ProtocolServer, ServerHandle};

use crate::agent_registry::AgentRegistry;
use crate::auth::{Auth0Config, Auth0Verifier, AuthProviderKind};
use crate::db::{Database, DbConfig};
use crate::handlers;
use crate::log_router::LogRouter;
use crate::server_provider::ServerProviderKind;

/// Shared application state for all channel handlers.
pub struct AppState {
    pub db: Database,
    /// 認証プロバイダ（OSS: NoAuth、SaaS: Auth0）
    pub auth: AuthProviderKind,
    /// クラウドプロバイダーのサーバー操作（オプション、未設定なら DB のみ操作）
    pub server_provider: Option<ServerProviderKind>,
    /// 接続中の Fleet Agent レジストリ
    pub agent_registry: AgentRegistry,
    /// コンテナログ Pub/Sub ルーター
    pub log_router: LogRouter,
}

/// Control Plane server configuration.
pub struct ServerConfig {
    pub listen_addr: String,
    pub db: DbConfig,
    /// Auth0 設定（None の場合は NoAuth = 認証なし）
    pub auth: Option<Auth0Config>,
    /// クラウドプロバイダーのサーバー操作（オプション）
    pub server_provider: Option<ServerProviderKind>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "[::1]:4510".into(),
            db: DbConfig::default(),
            auth: None,
            server_provider: None,
        }
    }
}

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("listen_addr", &self.listen_addr)
            .field("db", &self.db)
            .field("auth", &self.auth)
            .field(
                "server_provider",
                &self
                    .server_provider
                    .as_ref()
                    .map(|p| p.provider_name().to_string()),
            )
            .finish()
    }
}

/// Start the Control Plane server.
///
/// 1. Connect to SurrealDB
/// 2. Initialize Auth0 verifier
/// 3. Register Unison channels
/// 4. Start QUIC listener
pub async fn start(config: ServerConfig) -> Result<(ServerHandle, Arc<AppState>)> {
    // Initialize dependencies
    let db = Database::connect(&config.db).await?;
    let auth = match config.auth {
        Some(ref auth_config) => AuthProviderKind::Auth0(Auth0Verifier::new(auth_config)),
        None => {
            info!("Auth0 未設定 — NoAuth モード（認証なし）");
            AuthProviderKind::NoAuth
        }
    };

    let state = Arc::new(AppState {
        db,
        auth,
        server_provider: config.server_provider,
        agent_registry: AgentRegistry::new(),
        log_router: LogRouter::new(),
    });

    // Create Unison Protocol server
    let server = ProtocolServer::with_identity(
        "fleetflow-controlplane",
        env!("CARGO_PKG_VERSION"),
        "dev.fleetflow.controlplane",
    );

    // Register channels
    handlers::register_all(&server, state.clone()).await;

    info!(addr = %config.listen_addr, "Control Plane 起動中");

    // Start listening
    let handle = server
        .spawn_listen(&config.listen_addr)
        .await
        .context("QUIC リスナー起動失敗")?;

    info!(addr = %config.listen_addr, "Control Plane 起動完了");

    Ok((handle, state))
}
