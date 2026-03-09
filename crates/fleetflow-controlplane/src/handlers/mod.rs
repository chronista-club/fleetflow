pub mod container;
pub mod health;
pub mod project;
pub mod server;
pub mod service;
pub mod stage;
pub mod tenant;

use std::sync::Arc;
use unison::network::server::ProtocolServer;

use crate::server::AppState;

/// Register all channel handlers with the Unison Protocol server.
pub async fn register_all(server: &ProtocolServer, state: Arc<AppState>) {
    tenant::register(server, state.clone()).await;
    project::register(server, state.clone()).await;
    stage::register(server, state.clone()).await;
    service::register(server, state.clone()).await;
    container::register(server, state.clone()).await;
    self::server::register(server, state.clone()).await;
    health::register(server, state.clone()).await;
}
