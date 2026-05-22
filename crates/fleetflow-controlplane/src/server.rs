use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;
use unison::network::cert::CertSource;
use unison::network::quic::QuicServer;
use unison::network::server::ProtocolServer;

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
    /// CP server cert（MeshCa 発行）の SAN。
    ///
    /// クライアントが QUIC 接続に使うホスト名・IP（`cp.fleetstage.cloud` /
    /// Tailscale IP 等）。rustls の SAN 検証に一致する必要がある。
    pub cert_sans: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "[::1]:4510".into(),
            db: DbConfig::default(),
            auth: None,
            server_provider: None,
            cert_sans: vec!["localhost".into(), "::1".into()],
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
            .field("cert_sans", &self.cert_sans)
            .finish()
    }
}

/// Start the Control Plane server.
///
/// 1. SurrealDB に接続
/// 2. Auth0 verifier を初期化
/// 3. Unison channel を登録
/// 4. MeshCa をロード/生成し CP server cert を発行
/// 5. QUIC リスナーを起動
pub async fn start(config: ServerConfig) -> Result<(CpServerHandle, Arc<AppState>)> {
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

    // MeshCa — CA をロード/生成し、CP 自身の QUIC server cert を発行する。
    let ca = crate::cert::ensure_mesh_ca()?;
    let cert_source = crate::cert::issue_server_cert(&ca, &config.cert_sans)?;

    info!(
        addr = %config.listen_addr,
        sans = ?config.cert_sans,
        "Control Plane 起動中（MeshCa 発行 cert で QUIC TLS）"
    );

    let handle = spawn_quic(server, cert_source, &config.listen_addr).await?;

    info!(addr = %config.listen_addr, "Control Plane 起動完了");

    Ok((handle, state))
}

/// `QuicServer::builder` で CertSource を注入して QUIC server を spawn する。
///
/// unison の `ProtocolServer::spawn_listen` は raw QUIC で CertSource を受け取れず
/// `CertSource::dev_localhost`（DEV 用）に固定されるため、公開 builder API
/// `QuicServer::builder().cert_source(...)` を直接使う。
async fn spawn_quic(
    server: ProtocolServer,
    cert_source: CertSource,
    addr: &str,
) -> Result<CpServerHandle> {
    let protocol_server = Arc::new(server);
    let mut quic_server = QuicServer::builder(Arc::clone(&protocol_server))
        .cert_source(cert_source)
        .build();
    quic_server
        .bind(addr)
        .await
        .with_context(|| format!("QUIC bind 失敗: {addr}"))?;
    let local_addr = quic_server
        .local_addr()
        .context("QUIC server が bind されていません")?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let join_handle = tokio::spawn(async move {
        if let Err(e) = quic_server.start_with_shutdown(shutdown_rx).await {
            tracing::error!(error = %e, "QUIC server 異常終了");
        }
    });

    Ok(CpServerHandle {
        join_handle,
        shutdown_tx: Some(shutdown_tx),
        local_addr,
    })
}

/// CP QUIC server のライフサイクルハンドル。
///
/// unison の `ServerHandle` はフィールドが private で外部構築できない。
/// CertSource 注入のため `spawn_listen` でなく `QuicServer::builder` を使う
/// fleetflow では、同等のハンドルを自前で持つ。
pub struct CpServerHandle {
    join_handle: tokio::task::JoinHandle<()>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    local_addr: SocketAddr,
}

impl CpServerHandle {
    /// bind したローカルアドレス。
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// server タスクが終了済みか。
    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }

    /// graceful shutdown — shutdown シグナルを送り完了を待つ。
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.join_handle
            .await
            .context("CP server タスクの join 失敗")?;
        Ok(())
    }
}
