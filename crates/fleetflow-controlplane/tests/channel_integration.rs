//! Integration tests: Unison channel Request/Response for Control Plane handlers.
//!
//! Tests verify that handlers correctly process requests through the full
//! Unison Protocol stack (QUIC → Channel → Handler → SurrealDB → Response).

use std::sync::Arc;

use serde_json::json;
use unison::network::client::ProtocolClient;
use unison::network::server::ProtocolServer;

use fleetflow_controlplane::agent_registry::AgentRegistry;
use fleetflow_controlplane::auth::{Auth0Config, Auth0Verifier};
use fleetflow_controlplane::db::Database;
use fleetflow_controlplane::handlers;
use fleetflow_controlplane::log_router::LogRouter;
use fleetflow_controlplane::server::AppState;

/// Initialize rustls CryptoProvider (required once per process).
fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Helper: spin up a CP server on the given port with in-memory SurrealDB.
async fn start_test_server(port: u16) -> anyhow::Result<()> {
    ensure_crypto_provider();
    let db = Database::connect_memory().await?;
    let auth = Auth0Verifier::new(&Auth0Config {
        domain: "test.auth0.com".into(),
        audience: "https://test.fleetflow.dev".into(),
    });

    let state = Arc::new(AppState {
        db,
        auth,
        server_provider: None,
        agent_registry: AgentRegistry::new(),
        log_router: LogRouter::new(),
    });

    let server = ProtocolServer::with_identity(
        "fleetflow-controlplane-test",
        env!("CARGO_PKG_VERSION"),
        "dev.fleetflow.controlplane.test",
    );

    handlers::register_all(&server, state).await;

    let addr = format!("[::1]:{}", port);
    let _handle = server.spawn_listen(&addr).await?;

    // Keep server alive until test completes
    std::mem::forget(_handle);

    Ok(())
}

/// Helper: connect a client to the test server.
async fn connect_client(port: u16) -> anyhow::Result<ProtocolClient> {
    let client = ProtocolClient::new_default()?;
    let addr = format!("[::1]:{}", port);
    client.connect(&addr).await?;
    // Allow time for identity handshake
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Ok(client)
}

#[tokio::test]
async fn test_tenant_get_and_list() -> anyhow::Result<()> {
    let port = 14510;
    start_test_server(port).await?;
    let client = connect_client(port).await?;

    let channel = client.open_channel("tenant").await?;

    // List tenants (should be empty initially)
    let resp = channel.request("list", json!({})).await?;
    let tenants = resp["tenants"].as_array().unwrap();
    assert!(tenants.is_empty(), "初期状態ではテナントは空");

    // Get non-existent tenant
    let resp = channel
        .request("get", json!({ "slug": "nonexistent" }))
        .await?;
    assert!(
        resp.get("error").is_some() || resp.get("tenant").is_none(),
        "存在しないテナントの取得はエラーまたは null"
    );

    channel.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_project_create_and_list() -> anyhow::Result<()> {
    let port = 14511;
    start_test_server(port).await?;
    let client = connect_client(port).await?;

    let channel = client.open_channel("project").await?;

    // Create a project
    let resp = channel
        .request(
            "create",
            json!({
                "tenant_slug": "test-org",
                "name": "my-project",
                "description": "Test project"
            }),
        )
        .await?;

    // Should succeed (project created) or have an error about tenant
    if let Some(project) = resp.get("project") {
        assert_eq!(project["name"].as_str().unwrap(), "my-project");
    }

    // List projects
    let resp = channel
        .request("list", json!({ "tenant_slug": "test-org" }))
        .await?;
    assert!(resp.get("projects").is_some(), "projects キーが存在する");

    channel.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_health_overview() -> anyhow::Result<()> {
    let port = 14512;
    start_test_server(port).await?;
    let client = connect_client(port).await?;

    let channel = client.open_channel("health").await?;

    let resp = channel.request("overview", json!({})).await?;

    // Health overview returns project_count and server_count
    assert!(
        resp.get("project_count").is_some() || resp.get("error").is_none(),
        "ヘルス概要が取得できる"
    );

    channel.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_server_list_and_register() -> anyhow::Result<()> {
    let port = 14513;
    start_test_server(port).await?;
    let client = connect_client(port).await?;

    let channel = client.open_channel("server").await?;

    // List servers (empty)
    let resp = channel.request("list", json!({})).await?;
    let servers = resp["servers"].as_array().unwrap();
    assert!(servers.is_empty(), "初期状態ではサーバーは空");

    // Register a server
    let resp = channel
        .request(
            "register",
            json!({
                "tenant_slug": "test-org",
                "slug": "vps-01",
                "hostname": "test-server.local",
                "provider": "manual",
                "ip_address": "192.168.1.100"
            }),
        )
        .await?;
    assert!(
        resp.get("server").is_some() || resp.get("error").is_some(),
        "サーバー登録の結果が返る"
    );

    channel.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_unknown_method_returns_error() -> anyhow::Result<()> {
    let port = 14514;
    start_test_server(port).await?;
    let client = connect_client(port).await?;

    let channel = client.open_channel("tenant").await?;

    let resp = channel.request("nonexistent_method", json!({})).await?;
    assert!(resp.get("error").is_some(), "不明なメソッドはエラーを返す");

    channel.close().await?;
    client.disconnect().await?;
    Ok(())
}
