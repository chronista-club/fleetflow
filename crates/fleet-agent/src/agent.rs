//! Agent コアループ — CP 接続 + コマンド受信 + ハートビート

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use unison::network::channel::UnisonChannel;
use unison::network::client::ProtocolClient;

use crate::deploy;
use crate::heartbeat;
use crate::monitor;

pub struct AgentConfig {
    pub cp_endpoint: String,
    pub server_slug: String,
    pub heartbeat_interval_secs: u64,
    pub deploy_base: String,
    pub monitor_interval_secs: u64,
    pub restart_threshold: u32,
}

/// Agent メインループ
pub async fn run(config: AgentConfig) -> Result<()> {
    // Rustls CryptoProvider（Unison/QUIC に必要）
    let _ = rustls::crypto::ring::default_provider().install_default();

    loop {
        match run_session(&config).await {
            Ok(()) => {
                info!("セッション正常終了、再接続");
            }
            Err(e) => {
                error!(error = %e, "セッション異常終了、5秒後に再接続");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// 1回の CP 接続セッション
async fn run_session(config: &AgentConfig) -> Result<()> {
    let client = Arc::new(ProtocolClient::new_default().context("ProtocolClient 作成失敗")?);

    client
        .connect(&config.cp_endpoint)
        .await
        .with_context(|| format!("CP 接続失敗: {}", config.cp_endpoint))?;

    info!(endpoint = %config.cp_endpoint, "CP に接続完了");

    // Identity handshake 待機
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // ハートビートタスク起動
    let hb_client = Arc::clone(&client);
    let hb_slug = config.server_slug.clone();
    let hb_interval = config.heartbeat_interval_secs;
    let hb_handle = tokio::spawn(async move {
        heartbeat::run_loop(&hb_client, &hb_slug, hb_interval).await;
    });

    // モニタータスク起動
    let mon_client = Arc::clone(&client);
    let mon_slug = config.server_slug.clone();
    let mon_config = monitor::MonitorConfig {
        interval_secs: config.monitor_interval_secs,
        restart_threshold: config.restart_threshold,
        alert_cooldown_secs: 300,
    };
    let mon_handle = tokio::spawn(async move {
        monitor::run_loop(&mon_client, &mon_slug, &mon_config).await;
    });

    // コマンド受信ループ
    let result = command_loop(&client, &config.server_slug, &config.deploy_base).await;

    hb_handle.abort();
    mon_handle.abort();
    result
}

/// CP からのコマンドを待ち受けるループ
async fn command_loop(
    client: &Arc<ProtocolClient>,
    server_slug: &str,
    deploy_base: &str,
) -> Result<()> {
    let channel = client
        .open_channel("agent")
        .await
        .context("agent チャネルオープン失敗")?;

    // Agent 登録
    let register_resp: serde_json::Value = channel
        .request(
            "register",
            &json!({
                "server_slug": server_slug,
                "version": env!("CARGO_PKG_VERSION"),
            }),
        )
        .await
        .context("agent 登録失敗")?;

    let reg_status = register_resp["status"].as_str().unwrap_or("unknown");
    if reg_status != "ok" {
        anyhow::bail!("Agent 登録拒否: {register_resp}");
    }
    info!(response = %register_resp, "Agent 登録完了");

    // コマンド受信ループ
    loop {
        let msg = channel.recv().await?;
        let envelope = msg.payload_as_value()?;
        // CP (fleetflow-controlplane handlers/agent.rs) は AgentRegistry 経由の
        // 全コマンドを `{request_id, payload}` で wrap して send_event する。
        let (request_id, payload) = parse_command_envelope(&envelope);

        match msg.method.as_str() {
            "deploy" => {
                handle_deploy(&channel, request_id, &payload, deploy_base).await?;
            }
            "deploy.execute_kdl" => {
                // FSC-34: CP から flow.stages[stage].servers で routing された kdl-based deploy
                // (= bollard + DeployEngine 経由、 既存 docker-compose path とは別)
                handle_deploy_kdl(&channel, request_id, &payload).await?;
            }
            "restart" => {
                let service = payload["service"].as_str().unwrap_or("");
                let result = deploy::restart_service(service).await;
                let response = match result {
                    Ok(()) => json!({ "status": "ok" }),
                    Err(e) => json!({ "status": "failed", "error": e.to_string() }),
                };
                send_command_result(&channel, request_id, response).await?;
            }
            "status" => {
                let result = deploy::container_status().await;
                let response = match result {
                    Ok(v) => json!({ "status": "ok", "data": v }),
                    Err(e) => json!({ "status": "failed", "error": e.to_string() }),
                };
                send_command_result(&channel, request_id, response).await?;
            }
            "build" => {
                handle_build(&channel, request_id, &payload).await?;
            }
            "ping" => {
                send_command_result(&channel, request_id, json!({ "pong": true })).await?;
            }
            method => {
                warn!(method, "不明なコマンド");
                send_command_result(
                    &channel,
                    request_id,
                    json!({ "error": format!("unknown command: {}", method) }),
                )
                .await?;
            }
        }
    }
}

/// CP が送るコマンド envelope `{request_id, payload}` を分解する。
///
/// CP (`fleetflow-controlplane` handlers/agent.rs) は AgentRegistry 経由の
/// 全コマンドを `{"request_id": u64, "payload": <actual>}` で wrap して
/// `send_event` する。`request_id` は応答相関キー。
fn parse_command_envelope(envelope: &serde_json::Value) -> (u64, serde_json::Value) {
    let request_id = envelope["request_id"].as_u64().unwrap_or(0);
    let payload = envelope
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    (request_id, payload)
}

/// `command_result` の payload を組み立てる（純粋関数 — テスト可能）。
///
/// result object に `request_id` を merge して flat に返す。caller（CP の
/// deploy handler 等）は `resp["status"]` を top-level で読むため nest しない。
fn build_command_result_payload(
    request_id: u64,
    mut result: serde_json::Value,
) -> serde_json::Value {
    match result.as_object_mut() {
        Some(obj) => {
            obj.insert("request_id".to_string(), json!(request_id));
            result
        }
        // result が object でない異常系のみ wrap
        None => json!({ "request_id": request_id, "result": result }),
    }
}

/// コマンド応答を統一 method `command_result` で CP に返す。
///
/// CP は method `command_result` + payload の `request_id` で pending request に
/// 相関させる（handlers/agent.rs）。deploy/restart/status/build/ping 共通。
async fn send_command_result(
    channel: &UnisonChannel,
    request_id: u64,
    result: serde_json::Value,
) -> Result<()> {
    let payload = build_command_result_payload(request_id, result);
    channel.send_event("command_result", &payload).await?;
    Ok(())
}

/// デプロイコマンド処理 — ログを非同期ストリーミング送信しつつ最終結果を返す
async fn handle_deploy(
    channel: &UnisonChannel,
    request_id: u64,
    payload: &serde_json::Value,
    deploy_base: &str,
) -> Result<()> {
    // mpsc チャネルでログ行をストリーミング
    let (log_tx, mut log_rx) = mpsc::channel::<String>(256);

    // ログ行を受信し CP にイベント送信するタスク
    let channel_for_log = channel;
    let log_stream_task = {
        // ログ送信は別タスクにせず、select! で deploy と並行処理
        // （UnisonChannel は &self なので複数タスクで共有不可）
        // → deploy 完了後にドレインする方式
        async {
            let mut lines = Vec::new();
            while let Some(line) = log_rx.recv().await {
                lines.push(line);
            }
            lines
        }
    };

    // deploy 実行とログ受信を並行
    let (deploy_result, log_lines) = tokio::join!(
        deploy::execute(payload, deploy_base, Some(log_tx)),
        log_stream_task
    );

    // 蓄積されたログ行を CP に送信
    // D#8 fix: 旧実装は `let _ = ...send_event(...)` で silent discard していたため
    // CP channel が落ちている等で deploy log が全 loss しても何も log に出ず、
    // 運用 debug が困難だった。 失敗カウントを集計して warn 1 行で可視化。
    let mut failed_log_sends: usize = 0;
    for line in &log_lines {
        if channel_for_log
            .send_event(
                "deploy_log",
                &json!({
                    "request_id": request_id,
                    "line": line,
                }),
            )
            .await
            .is_err()
        {
            failed_log_sends += 1;
        }
    }
    if failed_log_sends > 0 {
        warn!(
            failed = failed_log_sends,
            total = log_lines.len(),
            "deploy log lines lost (CP channel error)"
        );
    }

    // 最終結果を Response として返す
    let log_count = log_lines.len();
    let response = match deploy_result {
        Ok(ref r) => json!({
            "status": r.status,
            "log_lines": log_count,
        }),
        Err(ref e) => json!({
            "status": "failed",
            "error": e.to_string(),
        }),
    };

    // deploy のビジネスエラーはレスポンスに含め、セッション切断しない
    // チャネル送信エラーのみ伝播（ネットワーク断）
    send_command_result(channel, request_id, response).await?;

    Ok(())
}

/// FSC-34: kdl-based deploy を local docker daemon で実行する handler。
///
/// CP の `deploy.execute` handler が `flow.stages[stage].servers` を resolve して
/// 該当 server slug の agent (= 自分) にこの method で routing する。 agent は
/// local docker daemon (= worker の dockerd) に bollard で接続し、 DeployEngine で
/// container を作成・起動する。
///
/// 既存 `deploy` method (= docker-compose 経由) とは **別系統**:
/// - `deploy`            : payload に compose_path を渡す、 docker-compose CLI spawn
/// - `deploy.execute_kdl`: payload に DeployRequest (Flow + stage_name + flags)、 bollard 直叩き
///
/// log streaming は将来課題 — 現状 deploy 完了まで block して結果を 1 response で返す。
/// CP 側 timeout を deploy の最大想定時間 (image pull 込み 10 分) に合わせること。
async fn handle_deploy_kdl(
    channel: &UnisonChannel,
    request_id: u64,
    payload: &serde_json::Value,
) -> Result<()> {
    use fleetflow_container::{DeployEngine, DeployRequest};

    // payload を DeployRequest に deserialize
    let deploy_request: DeployRequest = match serde_json::from_value(payload.clone()) {
        Ok(r) => r,
        Err(e) => {
            send_command_result(
                channel,
                request_id,
                json!({
                    "status": "failed",
                    "error": format!("invalid DeployRequest: {}", e),
                }),
            )
            .await?;
            return Ok(());
        }
    };

    // WS3: stage の backend が Quadlet なら bollard を使わず Quadlet 適用に分岐。
    // agent は CLI (commands/quadlet.rs) と同じ apply_stage を呼ぶ。
    if let Some(stage) = deploy_request.flow.stages.get(&deploy_request.stage_name) {
        match stage.backend {
            fleetflow_core::Backend::Quadlet => {
                let response = match fleetflow_container::quadlet::apply_stage(
                    &deploy_request.flow,
                    &deploy_request.stage_name,
                    stage,
                ) {
                    Ok(outcome) => json!({
                        "status": "success",
                        "log": format!(
                            "quadlet: {} units 反映, {} services 起動",
                            outcome.units_written,
                            outcome.services_started.len()
                        ),
                        "services_deployed": outcome.services_started,
                    }),
                    Err(e) => json!({
                        "status": "failed",
                        "error": format!("quadlet 適用失敗: {}", e),
                    }),
                };
                send_command_result(channel, request_id, response).await?;
                return Ok(());
            }
            fleetflow_core::Backend::Compose => {
                send_command_result(
                    channel,
                    request_id,
                    json!({
                        "status": "failed",
                        "error": "backend \"compose\" は未実装です（epic WS4）",
                    }),
                )
                .await?;
                return Ok(());
            }
            // Docker は以下の従来 bollard 経路へ
            fleetflow_core::Backend::Docker => {}
        }
    }

    // local docker daemon (= worker の dockerd) に bollard 接続
    let docker = match bollard::Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            send_command_result(
                channel,
                request_id,
                json!({
                    "status": "failed",
                    "error": format!("Docker 接続失敗: {}", e),
                }),
            )
            .await?;
            return Ok(());
        }
    };

    let engine = DeployEngine::new(docker);
    let log_buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log_buffer_clone = log_buffer.clone();

    let result = engine
        .execute(&deploy_request, move |event| {
            // event は Debug で文字列化、 後で 1 response として CP に返す
            log_buffer_clone
                .lock()
                .unwrap()
                .push(format!("{:?}", event));
        })
        .await;

    let response = match result {
        Ok(r) => {
            let combined = log_buffer.lock().unwrap().join("\n");
            json!({
                "status": "success",
                "log": combined + "\n" + &r.log.join("\n"),
                "services_deployed": r.services_deployed,
            })
        }
        Err(e) => json!({
            "status": "failed",
            "error": e.to_string(),
        }),
    };

    send_command_result(channel, request_id, response).await?;

    Ok(())
}

/// Build Tier v1 — "build" コマンドハンドラ
///
/// payload: { git_url, git_ref, dockerfile, image, job_id }
///
/// 1. work dir 作成 (/var/lib/fleet-agent/builds/<job-id>/)
/// 2. git clone
/// 3. fleetflow_build::ImageBuilder で docker build
/// 4. docker push (shellout)
/// 5. 完了 event を CP に送信
async fn handle_build(
    channel: &UnisonChannel,
    request_id: u64,
    payload: &serde_json::Value,
) -> Result<()> {
    let git_url = payload["git_url"].as_str().unwrap_or_default();
    let git_ref = payload["git_ref"].as_str().unwrap_or("main");
    let dockerfile = payload["dockerfile"].as_str().unwrap_or("Dockerfile");
    let image = payload["image"].as_str().unwrap_or_default();
    let job_id = payload["job_id"].as_str().unwrap_or("unknown");

    if git_url.is_empty() {
        send_command_result(
            channel,
            request_id,
            json!({ "status": "failed", "error": "git_url required" }),
        )
        .await?;
        return Ok(());
    }

    // work dir: /var/lib/fleet-agent/builds/<job-id>/
    let work_dir = std::path::PathBuf::from(format!("/var/lib/fleet-agent/builds/{}", job_id));
    if let Err(e) = std::fs::create_dir_all(&work_dir) {
        send_command_result(
            channel,
            request_id,
            json!({ "status": "failed", "error": format!("work dir 作成失敗: {}", e) }),
        )
        .await?;
        return Ok(());
    }

    // git clone
    info!(git_url, git_ref, job_id, "git clone 開始");
    let clone_result = std::process::Command::new("git")
        .args(["clone", "--depth=1", "--branch", git_ref, git_url, "."])
        .current_dir(&work_dir)
        .status();

    match clone_result {
        Ok(status) if status.success() => {
            info!(job_id, "git clone 完了");
        }
        Ok(status) => {
            let err = format!("git clone failed (exit code: {:?})", status.code());
            error!(job_id, error = %err, "git clone 失敗");
            send_command_result(
                channel,
                request_id,
                json!({ "status": "failed", "error": err }),
            )
            .await?;
            return Ok(());
        }
        Err(e) => {
            let err = format!("git clone shellout 失敗: {}", e);
            error!(job_id, error = %err);
            send_command_result(
                channel,
                request_id,
                json!({ "status": "failed", "error": err }),
            )
            .await?;
            return Ok(());
        }
    }

    // docker build via fleetflow-build ImageBuilder
    let start_ms = std::time::Instant::now();
    let docker = match bollard::Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            let err = format!("docker daemon 接続失敗: {}", e);
            error!(job_id, error = %err);
            send_command_result(
                channel,
                request_id,
                json!({ "status": "failed", "error": err }),
            )
            .await?;
            return Ok(());
        }
    };

    let builder = fleetflow_build::ImageBuilder::new(docker);
    let context_path = work_dir.clone();
    let dockerfile_path = work_dir.join(dockerfile);

    info!(job_id, image, "docker build 開始");
    let build_result = builder
        .build_image_from_path(
            &context_path,
            &dockerfile_path,
            image,
            HashMap::new(),
            None,
            false,
            Some("linux/amd64"),
        )
        .await;

    let duration_ms = start_ms.elapsed().as_millis();

    match build_result {
        Ok(()) => {
            info!(job_id, image, duration_ms, "docker build 成功");
        }
        Err(e) => {
            let err = format!("docker build 失敗: {}", e);
            error!(job_id, error = %err);
            send_command_result(
                channel,
                request_id,
                json!({ "status": "failed", "error": err, "duration_ms": duration_ms }),
            )
            .await?;
            return Ok(());
        }
    }

    // docker push (shellout)
    if !image.is_empty() {
        info!(job_id, image, "docker push 開始");
        let push_result = std::process::Command::new("docker")
            .args(["push", image])
            .status();

        match push_result {
            Ok(status) if status.success() => {
                info!(job_id, image, "docker push 完了");
            }
            Ok(status) => {
                let err = format!("docker push failed (exit code: {:?})", status.code());
                error!(job_id, error = %err);
                send_command_result(
                    channel,
                    request_id,
                    json!({ "status": "failed", "error": err, "duration_ms": duration_ms }),
                )
                .await?;
                return Ok(());
            }
            Err(e) => {
                let err = format!("docker push shellout 失敗: {}", e);
                error!(job_id, error = %err);
                send_command_result(
                    channel,
                    request_id,
                    json!({ "status": "failed", "error": err, "duration_ms": duration_ms }),
                )
                .await?;
                return Ok(());
            }
        }
    }

    // 成功
    let duration_secs = duration_ms / 1000;
    info!(job_id, image, duration_secs, "build 完了");
    send_command_result(
        channel,
        request_id,
        json!({
            "status": "success",
            "image": image,
            "duration_ms": duration_ms,
            "duration_seconds": duration_secs,
        }),
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_command_result_payload, parse_command_envelope};
    use serde_json::json;

    /// CP が送る正常な envelope `{request_id, payload}` を分解できる。
    #[test]
    fn parse_command_envelope_well_formed() {
        let envelope = json!({
            "request_id": 42,
            "payload": { "service": "web", "stage_name": "live" },
        });
        let (request_id, payload) = parse_command_envelope(&envelope);
        assert_eq!(request_id, 42);
        assert_eq!(payload, json!({ "service": "web", "stage_name": "live" }));
    }

    /// request_id 欠落時は 0（相関不能だが panic しない）、payload 欠落時は Null。
    #[test]
    fn parse_command_envelope_missing_fields() {
        let (rid, payload) = parse_command_envelope(&json!({}));
        assert_eq!(rid, 0);
        assert_eq!(payload, json!(null));
    }

    /// command_result は result object に request_id を flat に merge する
    /// （caller は resp["status"] を top-level で読むため nest しない）。
    #[test]
    fn build_command_result_payload_merges_request_id_flat() {
        let result = json!({ "status": "success", "log": "ok" });
        let payload = build_command_result_payload(7, result);
        assert_eq!(payload["request_id"], json!(7));
        assert_eq!(payload["status"], json!("success"));
        assert_eq!(payload["log"], json!("ok"));
    }

    /// result が object でない異常系は wrap して request_id を保つ。
    #[test]
    fn build_command_result_payload_wraps_non_object() {
        let payload = build_command_result_payload(9, json!("bare string"));
        assert_eq!(payload["request_id"], json!(9));
        assert_eq!(payload["result"], json!("bare string"));
    }

    /// CP↔agent 往復: CP が wrap した envelope を agent が分解し、
    /// agent の応答に CP の request_id がそのまま乗る（相関キーが保たれる）。
    #[test]
    fn command_envelope_roundtrip_preserves_request_id() {
        // CP 側 (handlers/agent.rs) の wrap 相当
        let cp_request_id = 123u64;
        let envelope = json!({
            "request_id": cp_request_id,
            "payload": { "dummy": true },
        });
        // agent 側で分解
        let (rid, _payload) = parse_command_envelope(&envelope);
        // agent が応答を組み立て
        let response = build_command_result_payload(rid, json!({ "status": "ok" }));
        // CP は response["request_id"] で pending request に相関させる
        assert_eq!(response["request_id"].as_u64(), Some(cp_request_id));
    }
}
