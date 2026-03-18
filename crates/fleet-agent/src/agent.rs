//! Agent コアループ — CP 接続 + コマンド受信 + ハートビート

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
