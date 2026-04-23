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
    let register_resp = channel
        .request(
            "register",
            json!({
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
        let payload = msg.payload_as_value()?;

        match msg.method.as_str() {
            "deploy" => {
                handle_deploy(&channel, msg.id, &payload, deploy_base).await?;
            }
            "restart" => {
                let service = payload["service"].as_str().unwrap_or("");
                let result = deploy::restart_service(service).await;
                let response = match result {
                    Ok(()) => json!({ "status": "ok" }),
                    Err(e) => json!({ "status": "failed", "error": e.to_string() }),
                };
                channel.send_response(msg.id, "restart", response).await?;
            }
            "status" => {
                let result = deploy::container_status().await;
                let response = match result {
                    Ok(v) => json!({ "status": "ok", "data": v }),
                    Err(e) => json!({ "status": "failed", "error": e.to_string() }),
                };
                channel.send_response(msg.id, "status", response).await?;
            }
            "build" => {
                handle_build(&channel, msg.id, &payload).await?;
            }
            "ping" => {
                channel
                    .send_response(msg.id, "ping", json!({ "pong": true }))
                    .await?;
            }
            method => {
                warn!(method, "不明なコマンド");
                channel
                    .send_response(
                        msg.id,
                        method,
                        json!({ "error": format!("unknown command: {}", method) }),
                    )
                    .await?;
            }
        }
    }
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
    for line in &log_lines {
        let _ = channel_for_log
            .send_event(
                "deploy_log",
                json!({
                    "request_id": request_id,
                    "line": line,
                }),
            )
            .await;
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
    channel
        .send_response(request_id, "deploy", response)
        .await?;

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
        channel
            .send_response(
                request_id,
                "build",
                json!({ "status": "failed", "error": "git_url required" }),
            )
            .await?;
        return Ok(());
    }

    // work dir: /var/lib/fleet-agent/builds/<job-id>/
    let work_dir = std::path::PathBuf::from(format!("/var/lib/fleet-agent/builds/{}", job_id));
    if let Err(e) = std::fs::create_dir_all(&work_dir) {
        channel
            .send_response(
                request_id,
                "build",
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
            channel
                .send_response(
                    request_id,
                    "build",
                    json!({ "status": "failed", "error": err }),
                )
                .await?;
            return Ok(());
        }
        Err(e) => {
            let err = format!("git clone shellout 失敗: {}", e);
            error!(job_id, error = %err);
            channel
                .send_response(
                    request_id,
                    "build",
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
            channel
                .send_response(
                    request_id,
                    "build",
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
            channel
                .send_response(
                    request_id,
                    "build",
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
                channel
                    .send_response(
                        request_id,
                        "build",
                        json!({ "status": "failed", "error": err, "duration_ms": duration_ms }),
                    )
                    .await?;
                return Ok(());
            }
            Err(e) => {
                let err = format!("docker push shellout 失敗: {}", e);
                error!(job_id, error = %err);
                channel
                    .send_response(
                        request_id,
                        "build",
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
    channel
        .send_response(
            request_id,
            "build",
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
