//! Agent チャネルハンドラー — Fleet Agent との双方向通信
//!
//! Agent が接続すると:
//! 1. register リクエストで AgentRegistry に登録
//! 2. select! で Agent からのメッセージと AgentRegistry からのコマンドを同時に待ち受け
//! 3. AgentRegistry からのコマンドを Agent に send_event で転送
//! 4. Agent からの応答を oneshot チャネルで呼び出し元に返す

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value, json};
use tracing::{error, info, warn};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::agent_registry::AgentCommand;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server
        .register_channel("agent", move |_ctx, stream| {
            let state = state.clone();
            Box::pin(async move {
                let channel = UnisonChannel::new(stream);

                // Step 1: Agent 登録（最初のメッセージは register であること）
                let msg = channel.recv().await?;
                let payload = msg.payload_as_value()?;

                if msg.method.as_str() != "register" {
                    warn!(method = %msg.method, "Agent の最初のメッセージが register ではない");
                    channel
                        .send_response(
                            msg.id,
                            &msg.method,
                            &json!({ "error": "first message must be 'register'" }),
                        )
                        .await?;
                    return Ok(());
                }

                let server_slug = payload["server_slug"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let version = payload["version"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();

                // AgentRegistry に登録 → コマンド受信用 rx を取得
                let mut command_rx = state
                    .agent_registry
                    .register(&server_slug, &version)
                    .await;

                // ハートビート更新
                state.db.update_server_heartbeat(&server_slug).await.ok();

                channel
                    .send_response(msg.id, "register", &json!({ "status": "ok" }))
                    .await?;

                info!(server = %server_slug, version = %version, "Agent 接続完了");

                // Step 2: select! で双方向通信
                // - Agent からのメッセージ（heartbeat, alert 等）
                // - AgentRegistry からのコマンド（deploy, restart 等 → Agent に転送）

                // 進行中のリクエスト ID → oneshot::Sender マッピング
                let mut pending_responses: HashMap<u64, tokio::sync::oneshot::Sender<Value>> =
                    HashMap::new();
                let mut next_request_id: u64 = 1;

                loop {
                    tokio::select! {
                        // Agent からのメッセージ
                        agent_msg = channel.recv() => {
                            match agent_msg {
                                Ok(m) => {
                                    let p = m.payload_as_value().unwrap_or_default();
                                    match m.method.as_str() {
                                        "heartbeat" => {
                                            let slug = p["server_slug"].as_str().unwrap_or(&server_slug);
                                            state.db.update_server_heartbeat(slug).await.ok();
                                            if let Some(pv) = p["agent_version"].as_str() {
                                                state.db.update_server_versions(slug, Some(pv), None).await.ok();
                                            }
                                            channel.send_response(m.id, "heartbeat", &json!({"status": "ok"})).await?;
                                        }
                                        "alert" => {
                                            // Agent からのアラート → DB に保存
                                            handle_agent_alert(&state, &p).await;
                                            channel.send_response(m.id, "alert", &json!({"status": "ok"})).await?;
                                        }
                                        "deploy_result" | "restart_result" | "status_result" => {
                                            // Agent からの応答 → pending_responses に返す
                                            let req_id = p["request_id"].as_u64().unwrap_or(0);
                                            if let Some(tx) = pending_responses.remove(&req_id) {
                                                let _ = tx.send(p);
                                            }
                                        }
                                        "log" => {
                                            // Agent からのコンテナログ → LogRouter に publish
                                            handle_agent_log(&state, &p).await;
                                        }
                                        method => {
                                            warn!(method, server = %server_slug, "Agent から不明なメッセージ");
                                        }
                                    }
                                }
                                Err(_) => {
                                    info!(server = %server_slug, "Agent 切断");
                                    break;
                                }
                            }
                        }
                        // AgentRegistry からのコマンド → Agent に転送
                        cmd = command_rx.recv() => {
                            match cmd {
                                Some(AgentCommand { method, payload, response_tx }) => {
                                    let req_id = next_request_id;
                                    next_request_id += 1;

                                    // response_tx がある場合は応答を待つ
                                    if let Some(tx) = response_tx {
                                        pending_responses.insert(req_id, tx);
                                    }

                                    // Agent にコマンドを送信
                                    let cmd_payload = json!({
                                        "request_id": req_id,
                                        "payload": payload,
                                    });

                                    if let Err(e) = channel.send_event(&method, &cmd_payload).await {
                                        error!(error = %e, method = %method, "Agent へのコマンド送信失敗");
                                        // 応答待ちがあればエラーを返す
                                        if let Some(tx) = pending_responses.remove(&req_id) {
                                            let _ = tx.send(json!({ "error": "send failed" }));
                                        }
                                    }
                                }
                                None => {
                                    // AgentRegistry がドロップ（通常は起こらない）
                                    break;
                                }
                            }
                        }
                    }
                }

                // クリーンアップ
                state.agent_registry.unregister(&server_slug).await;
                info!(server = %server_slug, "Agent セッション終了");

                Ok(())
            })
        })
        .await;
}

/// Agent からのコンテナログを LogRouter に publish
async fn handle_agent_log(state: &AppState, payload: &Value) {
    use crate::log_router::LogEntry;

    let server_slug = payload["server_slug"].as_str().unwrap_or("");
    let container_name = payload["container_name"].as_str().unwrap_or("");
    let stream = payload["stream"].as_str().unwrap_or("stdout");
    let level = payload["level"].as_str().unwrap_or("info");
    let message = payload["message"].as_str().unwrap_or("");

    let timestamp = payload["timestamp"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    state
        .log_router
        .publish(LogEntry {
            timestamp,
            server_slug: server_slug.to_string(),
            container_name: container_name.to_string(),
            stream: stream.to_string(),
            level: level.to_string(),
            message: message.to_string(),
        })
        .await;
}

/// Agent からのアラートを DB に保存
async fn handle_agent_alert(state: &AppState, payload: &Value) {
    let server_slug = payload["server_slug"].as_str().unwrap_or("");
    let container_name = payload["container_name"].as_str().unwrap_or("");
    let alert_type = payload["alert_type"].as_str().unwrap_or("unknown");
    let severity = payload["severity"].as_str().unwrap_or("warning");
    let message = payload["message"].as_str().unwrap_or("");

    // テナント解決（server_slug → tenant）
    let server = match state.db.get_server_by_slug(server_slug).await {
        Ok(Some(s)) => s,
        _ => {
            warn!(server = server_slug, "アラート: サーバーが見つからない");
            return;
        }
    };

    let alert = crate::model::Alert {
        id: None,
        tenant: server.tenant,
        server_slug: server_slug.to_string(),
        container_name: container_name.to_string(),
        alert_type: alert_type.to_string(),
        severity: severity.to_string(),
        message: message.to_string(),
        resolved: false,
        resolved_at: None,
        created_at: None,
    };

    match state.db.upsert_alert(&alert).await {
        Ok(_) => info!(
            server = server_slug,
            container = container_name,
            alert_type,
            "アラート保存"
        ),
        Err(e) => error!(error = %e, "アラート保存失敗"),
    }
}
