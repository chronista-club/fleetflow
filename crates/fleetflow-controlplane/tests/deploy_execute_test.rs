//! Large テスト: CP deploy.execute チャネルの統合テスト
//!
//! kv-mem SurrealDB + Unison Protocol を使って、
//! deploy.execute リクエストが Deployment レコードを作成することを検証する。

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use unison::network::client::ProtocolClient;
use unison::network::server::ProtocolServer;

use fleetflow_controlplane::auth::{Auth0Config, Auth0Verifier};
use fleetflow_controlplane::db::Database;
use fleetflow_controlplane::handlers;
use fleetflow_controlplane::server::AppState;

use fleetflow_core::{Flow, Service, Stage};

fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

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

fn make_test_flow() -> Flow {
    let mut services = HashMap::new();
    services.insert(
        "test-svc".to_string(),
        Service {
            image: Some("alpine:latest".into()),
            command: Some("echo hello".into()),
            ..Default::default()
        },
    );

    let mut stages = HashMap::new();
    stages.insert(
        "test".to_string(),
        Stage {
            services: vec!["test-svc".to_string()],
            servers: vec![],
            variables: HashMap::new(),
            registry: None,
        },
    );

    Flow {
        name: "deploy-test".to_string(),
        services,
        stages,
        providers: HashMap::new(),
        servers: HashMap::new(),
        registry: None,
        variables: HashMap::new(),
    }
}

/// CP に deploy.execute リクエストを送り、レスポンスが返ることを検証。
///
/// Note: tenant/project が kv-mem に存在しないためエラーレスポンスが返るが、
/// チャネルが正しく execute メソッドをルーティングしてることを検証する。
#[tokio::test]
async fn test_deploy_execute_channel() -> anyhow::Result<()> {
    let port = 14520;
    start_test_server(port).await?;
    let client = connect_client(port).await?;

    let channel = client.open_channel("deploy").await?;

    let flow = make_test_flow();
    let request = fleetflow_container::DeployRequest {
        flow,
        stage_name: "test".into(),
        target_services: vec!["test-svc".into()],
        no_pull: true,
        no_prune: true,
    };

    let resp = channel
        .request(
            "execute",
            json!({
                "tenant_slug": "default",
                "project_slug": "deploy-test",
                "request": serde_json::to_value(&request)?,
            }),
        )
        .await?;

    // tenant "default" は kv-mem には存在しないので "tenant not found" エラーが返る
    assert!(
        resp.get("error").is_some(),
        "テナントが存在しない場合はエラーが返る: {:?}",
        resp
    );
    assert_eq!(
        resp["error"].as_str().unwrap(),
        "tenant not found",
        "エラーメッセージが正しい"
    );

    channel.close().await?;
    client.disconnect().await?;
    Ok(())
}

/// テナントとプロジェクトを事前作成してから deploy.execute を実行。
/// Docker が利用可能なら成功、利用不可ならDocker接続エラーが返る。
#[tokio::test]
async fn test_deploy_execute_with_tenant_and_project() -> anyhow::Result<()> {
    let port = 14521;
    start_test_server(port).await?;
    let client = connect_client(port).await?;

    // テナント作成
    let tenant_ch = client.open_channel("tenant").await?;
    let resp = tenant_ch
        .request(
            "create",
            json!({ "name": "Test Org", "slug": "test-deploy-org" }),
        )
        .await?;
    tenant_ch.close().await?;
    // テナントが作成されたことを確認（エラーがないこと）
    assert!(
        resp.get("error").is_none(),
        "テナント作成が成功: {:?}",
        resp
    );

    // プロジェクト作成
    let project_ch = client.open_channel("project").await?;
    let resp = project_ch
        .request(
            "create",
            json!({
                "tenant_slug": "test-deploy-org",
                "slug": "deploy-test",
                "name": "deploy-test",
                "description": "test project for deploy"
            }),
        )
        .await?;
    project_ch.close().await?;
    assert!(
        resp.get("error").is_none(),
        "プロジェクト作成が成功: {:?}",
        resp
    );

    // deploy.execute
    let deploy_ch = client.open_channel("deploy").await?;
    let flow = make_test_flow();
    let request = fleetflow_container::DeployRequest {
        flow,
        stage_name: "test".into(),
        target_services: vec!["test-svc".into()],
        no_pull: true,
        no_prune: true,
    };

    let resp = deploy_ch
        .request(
            "execute",
            json!({
                "tenant_slug": "test-deploy-org",
                "project_slug": "deploy-test",
                "request": serde_json::to_value(&request)?,
            }),
        )
        .await?;
    deploy_ch.close().await?;

    // Docker が利用可能な環境では success、利用不可なら Docker connection error
    if let Some(err) = resp.get("error").and_then(|e| e.as_str()) {
        assert!(err.contains("Docker"), "Docker 関連のエラーが返る: {}", err);
    } else {
        // Docker 利用可能: deployment_id と status が返る
        assert!(resp.get("deployment_id").is_some(), "deployment_id が返る");
        assert!(resp.get("status").is_some(), "status が返る");
    }

    client.disconnect().await?;
    Ok(())
}
