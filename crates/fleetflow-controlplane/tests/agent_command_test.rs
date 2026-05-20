//! P0 回帰テスト: CP↔agent コマンド応答プロトコルの往復。
//!
//! バグ: CP は agent コマンドの応答を method `deploy_result`/`restart_result`/
//! `status_result` + payload の `request_id` で相関させていたが、agent は
//! `send_response(msg.id, "<command>", …)`（method 名不一致・request_id 欠落）で
//! 応答していた → CP の oneshot が永久に未解決 → `send_command_with_timeout` が
//! timeout。修正: agent は統一 method `command_result` + flat `request_id` で応答。
//!
//! 本テストは修正後 agent と同じ contract の fake agent を接続し、
//! `send_command_with_timeout` が timeout せず即 Ok で返ることを確認する。
//! 旧コードでは `command_result` が drop され 5s フル timeout で fail する。

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use unison::network::client::ProtocolClient;
use unison::network::server::ProtocolServer;

use fleetflow_controlplane::agent_registry::AgentRegistry;
use fleetflow_controlplane::auth::AuthProviderKind;
use fleetflow_controlplane::db::Database;
use fleetflow_controlplane::handlers;
use fleetflow_controlplane::log_router::LogRouter;
use fleetflow_controlplane::server::AppState;

fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// CP サーバを起動し、`agent_registry` にアクセスするため `Arc<AppState>` を返す。
async fn start_cp(port: u16) -> anyhow::Result<Arc<AppState>> {
    ensure_crypto_provider();
    let db = Database::connect_memory().await?;
    let state = Arc::new(AppState {
        db,
        auth: AuthProviderKind::NoAuth,
        server_provider: None,
        agent_registry: AgentRegistry::new(),
        log_router: LogRouter::new(),
    });

    let server = ProtocolServer::with_identity(
        "fleetflow-controlplane-test",
        env!("CARGO_PKG_VERSION"),
        "dev.fleetflow.controlplane.test",
    );
    handlers::register_all(&server, state.clone()).await;

    let handle = server.spawn_listen(&format!("[::1]:{port}")).await?;
    std::mem::forget(handle);
    Ok(state)
}

#[tokio::test]
async fn test_agent_command_roundtrip_returns_via_command_result() -> anyhow::Result<()> {
    let port = 14550;
    let state = start_cp(port).await?;

    // fake agent: `agent` チャネルに接続して register
    let agent_client = ProtocolClient::new_default()?;
    agent_client.connect(&format!("[::1]:{port}")).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let agent_channel = agent_client.open_channel("agent").await?;
    let reg: Value = agent_channel
        .request(
            "register",
            &json!({ "server_slug": "test-worker", "version": "test" }),
        )
        .await?;
    assert_eq!(reg["status"], "ok", "agent 登録成功");

    // fake agent ループ: コマンド event を受け、修正後 agent と同じ contract
    // （統一 method `command_result` + flat `request_id`）で応答する。
    let agent_task = tokio::spawn(async move {
        loop {
            let msg = match agent_channel.recv().await {
                Ok(m) => m,
                Err(_) => break,
            };
            let envelope = msg.payload_as_value().unwrap_or_default();
            let request_id = envelope["request_id"].as_u64().unwrap_or(0);
            let _ = agent_channel
                .send_event(
                    "command_result",
                    &json!({
                        "request_id": request_id,
                        "status": "success",
                        "log": "roundtrip ok",
                    }),
                )
                .await;
        }
    });

    // agent task が recv() に入るのを待つ
    tokio::time::sleep(Duration::from_millis(100)).await;

    // CP → agent コマンド送信。5s timeout: 旧バグなら command_result が drop され
    // 5s フルに待って Err(timeout)。修正後は即 Ok。
    let resp = state
        .agent_registry
        .send_command_with_timeout(
            "test-worker",
            "deploy.execute_kdl",
            json!({ "dummy": true }),
            Duration::from_secs(5),
        )
        .await;

    agent_task.abort();

    let resp = resp.map_err(|e| anyhow::anyhow!("command roundtrip failed: {e}"))?;
    assert_eq!(resp["status"], "success", "agent 応答が CP に届く");
    assert_eq!(resp["log"], "roundtrip ok");
    assert_eq!(
        resp["request_id"].as_u64(),
        Some(1),
        "最初の request_id は 1"
    );

    drop(agent_client);
    Ok(())
}
