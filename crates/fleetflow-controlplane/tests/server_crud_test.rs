//! Large テスト: server チャネルの CRUD 統合テスト
//!
//! kv-mem SurrealDB + Unison Protocol + MockServerProvider を使って、
//! server チャネルの get/create/delete/power-on/power-off を検証する。

use std::sync::Arc;

use serde_json::json;
use unison::network::client::ProtocolClient;
use unison::network::server::ProtocolServer;

use fleetflow_controlplane::auth::{Auth0Config, Auth0Verifier};
use fleetflow_controlplane::db::Database;
use fleetflow_controlplane::handlers;
use fleetflow_controlplane::server::AppState;
use fleetflow_controlplane::server_provider::{MockServerProvider, ServerProviderKind};

fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

// ─────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────

async fn start_test_server_with_provider(
    port: u16,
    server_provider: Option<ServerProviderKind>,
) -> anyhow::Result<()> {
    ensure_crypto_provider();
    let db = Database::connect_memory().await?;
    let auth = Auth0Verifier::new(&Auth0Config {
        domain: "test.auth0.com".into(),
        audience: "https://test.fleetflow.dev".into(),
    });

    let state = Arc::new(AppState {
        db,
        auth,
        server_provider,
    });

    let server = ProtocolServer::with_identity(
        "fleetflow-controlplane-test",
        env!("CARGO_PKG_VERSION"),
        "dev.fleetflow.controlplane.test",
    );

    handlers::register_all(&server, state).await;

    let addr = format!("[::1]:{}", port);
    let _handle = server.spawn_listen(&addr).await?;
    std::mem::forget(_handle);

    Ok(())
}

async fn connect_client(port: u16) -> anyhow::Result<ProtocolClient> {
    let client = ProtocolClient::new_default()?;
    let addr = format!("[::1]:{}", port);
    client.connect(&addr).await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Ok(client)
}

async fn create_tenant(client: &ProtocolClient, slug: &str) -> anyhow::Result<()> {
    let ch = client.open_channel("tenant").await?;
    let resp = ch
        .request("create", json!({ "name": slug, "slug": slug }))
        .await?;
    ch.close().await?;
    assert!(resp.get("error").is_none(), "テナント作成失敗: {:?}", resp);
    Ok(())
}

fn mock_provider() -> (
    ServerProviderKind,
    std::sync::Arc<std::sync::Mutex<Vec<fleetflow_controlplane::server_provider::MockCall>>>,
) {
    let (mock, calls) = MockServerProvider::new();
    (ServerProviderKind::Mock(mock), calls)
}

// ─────────────────────────────────────────
// Tests: server.get
// ─────────────────────────────────────────

#[tokio::test]
async fn test_server_get_not_found() -> anyhow::Result<()> {
    let port = 14530;
    start_test_server_with_provider(port, None).await?;
    let client = connect_client(port).await?;

    let ch = client.open_channel("server").await?;
    let resp = ch.request("get", json!({ "slug": "nonexistent" })).await?;

    assert_eq!(resp["error"].as_str().unwrap(), "server not found");

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_server_get_after_register() -> anyhow::Result<()> {
    let port = 14531;
    start_test_server_with_provider(port, None).await?;
    let client = connect_client(port).await?;

    create_tenant(&client, "get-test-org").await?;

    let ch = client.open_channel("server").await?;

    ch.request(
        "register",
        json!({
            "tenant_slug": "get-test-org",
            "slug": "web-01",
            "provider": "manual",
            "ssh_host": "192.168.1.100",
            "ssh_user": "root",
            "deploy_path": "/opt/app"
        }),
    )
    .await?;

    let resp = ch.request("get", json!({ "slug": "web-01" })).await?;
    assert!(resp.get("error").is_none(), "get 成功: {:?}", resp);
    assert_eq!(resp["server"]["slug"].as_str().unwrap(), "web-01");
    assert_eq!(resp["server"]["provider"].as_str().unwrap(), "manual");

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}

// ─────────────────────────────────────────
// Tests: server.create
// ─────────────────────────────────────────

#[tokio::test]
async fn test_server_create_no_provider() -> anyhow::Result<()> {
    let port = 14532;
    start_test_server_with_provider(port, None).await?;
    let client = connect_client(port).await?;

    let ch = client.open_channel("server").await?;
    let resp = ch
        .request(
            "create",
            json!({
                "tenant_slug": "some-org",
                "request": {
                    "name": "test-server",
                    "cpu": 2,
                    "memory_gb": 4,
                    "ssh_keys": [],
                    "tags": []
                }
            }),
        )
        .await?;

    assert_eq!(
        resp["error"].as_str().unwrap(),
        "server provider not configured"
    );

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_server_create_tenant_not_found() -> anyhow::Result<()> {
    let port = 14533;
    let (provider, _calls) = mock_provider();
    start_test_server_with_provider(port, Some(provider)).await?;
    let client = connect_client(port).await?;

    let ch = client.open_channel("server").await?;
    let resp = ch
        .request(
            "create",
            json!({
                "tenant_slug": "nonexistent-org",
                "request": {
                    "name": "test-server",
                    "cpu": 2,
                    "memory_gb": 4,
                    "ssh_keys": [],
                    "tags": []
                }
            }),
        )
        .await?;

    assert_eq!(resp["error"].as_str().unwrap(), "tenant not found");

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_server_create_success() -> anyhow::Result<()> {
    let port = 14534;
    let (provider, calls) = mock_provider();
    start_test_server_with_provider(port, Some(provider)).await?;
    let client = connect_client(port).await?;

    create_tenant(&client, "create-test-org").await?;

    let ch = client.open_channel("server").await?;
    let resp = ch
        .request(
            "create",
            json!({
                "tenant_slug": "create-test-org",
                "request": {
                    "name": "fleet-worker-01",
                    "cpu": 2,
                    "memory_gb": 4,
                    "disk_gb": 40,
                    "os_type": "debian",
                    "ssh_keys": ["my-key"],
                    "tags": ["fleetflow", "worker"]
                }
            }),
        )
        .await?;

    assert!(resp.get("error").is_none(), "create 成功: {:?}", resp);
    assert_eq!(resp["cloud"]["id"].as_str().unwrap(), "mock-12345");
    assert_eq!(resp["cloud"]["name"].as_str().unwrap(), "fleet-worker-01");
    assert_eq!(resp["server"]["slug"].as_str().unwrap(), "fleet-worker-01");

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}

// ─────────────────────────────────────────
// Tests: server.delete
// ─────────────────────────────────────────

#[tokio::test]
async fn test_server_delete_db_only() -> anyhow::Result<()> {
    let port = 14535;
    start_test_server_with_provider(port, None).await?;
    let client = connect_client(port).await?;

    create_tenant(&client, "delete-test-org").await?;

    let ch = client.open_channel("server").await?;

    ch.request(
        "register",
        json!({
            "tenant_slug": "delete-test-org",
            "slug": "to-delete",
            "provider": "manual",
            "ssh_host": "10.0.0.1",
        }),
    )
    .await?;

    let resp = ch.request("delete", json!({ "slug": "to-delete" })).await?;
    assert!(resp.get("error").is_none(), "delete 成功: {:?}", resp);
    assert_eq!(resp["deleted"].as_str().unwrap(), "to-delete");

    let resp = ch.request("get", json!({ "slug": "to-delete" })).await?;
    assert_eq!(resp["error"].as_str().unwrap(), "server not found");

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_server_delete_with_cloud() -> anyhow::Result<()> {
    let port = 14536;
    let (provider, calls) = mock_provider();
    start_test_server_with_provider(port, Some(provider)).await?;
    let client = connect_client(port).await?;

    create_tenant(&client, "delete-cloud-org").await?;

    let ch = client.open_channel("server").await?;

    ch.request(
        "register",
        json!({
            "tenant_slug": "delete-cloud-org",
            "slug": "cloud-server",
            "provider": "mock-provider",
            "ssh_host": "10.0.0.2",
        }),
    )
    .await?;

    let resp = ch
        .request(
            "delete",
            json!({
                "slug": "cloud-server",
                "cloud_id": "99999",
                "with_disks": true
            }),
        )
        .await?;
    assert!(resp.get("error").is_none(), "delete 成功: {:?}", resp);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}

// ─────────────────────────────────────────
// Tests: server.power-on / power-off
// ─────────────────────────────────────────

#[tokio::test]
async fn test_server_power_on_no_provider() -> anyhow::Result<()> {
    let port = 14537;
    start_test_server_with_provider(port, None).await?;
    let client = connect_client(port).await?;

    let ch = client.open_channel("server").await?;
    let resp = ch
        .request("power-on", json!({ "cloud_id": "12345" }))
        .await?;

    assert_eq!(
        resp["error"].as_str().unwrap(),
        "server provider not configured"
    );

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_server_power_on_success() -> anyhow::Result<()> {
    let port = 14538;
    let (provider, calls) = mock_provider();
    start_test_server_with_provider(port, Some(provider)).await?;
    let client = connect_client(port).await?;

    let ch = client.open_channel("server").await?;
    let resp = ch
        .request("power-on", json!({ "cloud_id": "12345" }))
        .await?;

    assert!(resp.get("error").is_none(), "power-on 成功: {:?}", resp);
    assert_eq!(resp["ok"].as_bool().unwrap(), true);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn test_server_power_off_success() -> anyhow::Result<()> {
    let port = 14539;
    let (provider, calls) = mock_provider();
    start_test_server_with_provider(port, Some(provider)).await?;
    let client = connect_client(port).await?;

    let ch = client.open_channel("server").await?;
    let resp = ch
        .request("power-off", json!({ "cloud_id": "67890" }))
        .await?;

    assert!(resp.get("error").is_none(), "power-off 成功: {:?}", resp);
    assert_eq!(resp["ok"].as_bool().unwrap(), true);

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);

    ch.close().await?;
    client.disconnect().await?;
    Ok(())
}
