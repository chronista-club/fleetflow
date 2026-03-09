use std::sync::Arc;

use serde_json::json;
use tracing::info;
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server
        .register_channel("container", move |_ctx, stream| {
            let state = state.clone();
            Box::pin(async move {
                let channel = UnisonChannel::new(stream);
                loop {
                    let msg = channel.recv().await?;
                    let payload: serde_json::Value =
                        serde_json::from_str(&msg.payload).unwrap_or_default();

                    match msg.method.as_str() {
                        "start" => {
                            let container_name =
                                payload["container_name"].as_str().unwrap_or_default();

                            if container_name.is_empty() {
                                channel
                                    .send_response(
                                        msg.id,
                                        "start",
                                        json!({ "error": "container_name is required" }),
                                    )
                                    .await?;
                                continue;
                            }

                            let result = start_container(&state, container_name).await;
                            let resp = match result {
                                Ok(()) => json!({ "status": "started", "container_name": container_name }),
                                Err(e) => json!({ "error": e.to_string() }),
                            };
                            channel.send_response(msg.id, "start", resp).await?;
                        }
                        "stop" => {
                            let container_name =
                                payload["container_name"].as_str().unwrap_or_default();

                            if container_name.is_empty() {
                                channel
                                    .send_response(
                                        msg.id,
                                        "stop",
                                        json!({ "error": "container_name is required" }),
                                    )
                                    .await?;
                                continue;
                            }

                            let result = stop_container(&state, container_name).await;
                            let resp = match result {
                                Ok(()) => json!({ "status": "stopped", "container_name": container_name }),
                                Err(e) => json!({ "error": e.to_string() }),
                            };
                            channel.send_response(msg.id, "stop", resp).await?;
                        }
                        "restart" => {
                            let container_name =
                                payload["container_name"].as_str().unwrap_or_default();

                            if container_name.is_empty() {
                                channel
                                    .send_response(
                                        msg.id,
                                        "restart",
                                        json!({ "error": "container_name is required" }),
                                    )
                                    .await?;
                                continue;
                            }

                            let result = restart_container(&state, container_name).await;
                            let resp = match result {
                                Ok(()) => json!({ "status": "restarted", "container_name": container_name }),
                                Err(e) => json!({ "error": e.to_string() }),
                            };
                            channel.send_response(msg.id, "restart", resp).await?;
                        }
                        "logs" => {
                            // TODO: Event push でログストリーミング
                            // 現在は直近のログを一括返却
                            let container_name =
                                payload["container_name"].as_str().unwrap_or_default();
                            let tail = payload["tail"].as_u64().unwrap_or(100);

                            let result = get_logs(&state, container_name, tail).await;
                            let resp = match result {
                                Ok(logs) => json!({ "logs": logs, "container_name": container_name }),
                                Err(e) => json!({ "error": e.to_string() }),
                            };
                            channel.send_response(msg.id, "logs", resp).await?;
                        }
                        "exec" => {
                            // TODO: Phase 2 — Raw bytes で stdin/stdout 双方向通信
                            channel
                                .send_response(
                                    msg.id,
                                    "exec",
                                    json!({ "error": "exec は Phase 2 で実装予定" }),
                                )
                                .await?;
                        }
                        method => {
                            info!(method, "container: 不明なメソッド");
                            channel
                                .send_response(
                                    msg.id,
                                    method,
                                    json!({ "error": format!("unknown method: {}", method) }),
                                )
                                .await?;
                        }
                    }
                }
            })
        })
        .await;
}

/// Docker コンテナを起動
async fn start_container(_state: &AppState, container_name: &str) -> anyhow::Result<()> {
    let docker = bollard::Docker::connect_with_local_defaults()?;
    docker
        .start_container(container_name, None::<bollard::query_parameters::StartContainerOptions>)
        .await
        .map_err(|e| anyhow::anyhow!("コンテナ起動失敗 ({}): {}", container_name, e))?;
    info!(container = container_name, "コンテナ起動");
    Ok(())
}

/// Docker コンテナを停止
async fn stop_container(_state: &AppState, container_name: &str) -> anyhow::Result<()> {
    let docker = bollard::Docker::connect_with_local_defaults()?;
    docker
        .stop_container(container_name, None::<bollard::query_parameters::StopContainerOptions>)
        .await
        .map_err(|e| anyhow::anyhow!("コンテナ停止失敗 ({}): {}", container_name, e))?;
    info!(container = container_name, "コンテナ停止");
    Ok(())
}

/// Docker コンテナを再起動
async fn restart_container(_state: &AppState, container_name: &str) -> anyhow::Result<()> {
    let docker = bollard::Docker::connect_with_local_defaults()?;
    docker
        .restart_container(container_name, None::<bollard::query_parameters::RestartContainerOptions>)
        .await
        .map_err(|e| anyhow::anyhow!("コンテナ再起動失敗 ({}): {}", container_name, e))?;
    info!(container = container_name, "コンテナ再起動");
    Ok(())
}

/// Docker コンテナのログ取得
async fn get_logs(
    _state: &AppState,
    container_name: &str,
    tail: u64,
) -> anyhow::Result<Vec<String>> {
    use futures_util::StreamExt;

    let docker = bollard::Docker::connect_with_local_defaults()?;

    let options = bollard::query_parameters::LogsOptions {
        stdout: true,
        stderr: true,
        tail: tail.to_string(),
        ..Default::default()
    };

    let mut log_stream = docker.logs(container_name, Some(options));
    let mut lines = Vec::new();

    while let Some(Ok(output)) = log_stream.next().await {
        lines.push(output.to_string());
        if lines.len() >= tail as usize {
            break;
        }
    }

    Ok(lines)
}
