use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;
use unison::network::server::{ProtocolServer, ServerHandle};

use crate::auth::{Auth0Config, Auth0Verifier};
use crate::db::{Database, DbConfig};
use crate::handlers;

/// Shared application state for all channel handlers.
pub struct AppState {
    pub db: Database,
    pub auth: Auth0Verifier,
}

/// Control Plane server configuration.
#[derive(Debug)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub db: DbConfig,
    pub auth: Auth0Config,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "[::1]:4510".into(),
            db: DbConfig::default(),
            auth: Auth0Config {
                domain: "anycreative.auth0.com".into(),
                audience: "https://api.fleetflow.dev".into(),
            },
        }
    }
}

/// Start the Control Plane server.
///
/// 1. Connect to SurrealDB
/// 2. Initialize Auth0 verifier
/// 3. Register Unison channels
/// 4. Start QUIC listener
pub async fn start(config: ServerConfig) -> Result<ServerHandle> {
    // Initialize dependencies
    let db = Database::connect(&config.db).await?;
    let auth = Auth0Verifier::new(&config.auth);

    let state = Arc::new(AppState { db, auth });

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

    Ok(handle)
}
