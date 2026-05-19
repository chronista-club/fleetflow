pub mod agent;
pub mod build;
pub mod container;
pub mod cost;
pub mod deploy;
pub mod dns;
pub mod health;
pub mod project;
pub mod server;
pub mod service;
pub mod stage;
pub mod tenant;
pub mod volume;

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
    cost::register(server, state.clone()).await;
    dns::register(server, state.clone()).await;
    deploy::register(server, state.clone()).await;
    agent::register(server, state.clone()).await;
    volume::register(server, state.clone()).await;
    build::register(server, state.clone()).await;
}
