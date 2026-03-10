use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::Deployment;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server
        .register_channel("deploy", move |_ctx, stream| {
            let state = state.clone();
            Box::pin(async move {
                let channel = UnisonChannel::new(stream);
                loop {
                    let msg = channel.recv().await?;
                    let payload = msg.payload_as_value()?;

                    match msg.method.as_str() {
                        "run" => {
                            let tenant_slug =
                                payload["tenant_slug"].as_str().unwrap_or("default");

                            // 必須フィールドのバリデーション
                            macro_rules! require_field {
                                ($field:expr, $name:literal) => {
                                    match payload[$field].as_str().filter(|s| !s.is_empty()) {
                                        Some(v) => v,
                                        None => {
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "run",
                                                    json!({ "error": format!("missing required field: {}", $name) }),
                                                )
                                                .await?;
                                            continue;
                                        }
                                    }
                                };
                            }

                            let project_slug = require_field!("project_slug", "project_slug");
                            let stage = require_field!("stage", "stage");
                            let server_slug = require_field!("server_slug", "server_slug");
                            let command = require_field!("command", "command");

                            // Resolve tenant
                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "run",
                                            json!({ "error": "tenant not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "tenant lookup 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "run",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            // Resolve project
                            let project = match state
                                .db
                                .get_project_by_slug(tenant_slug, project_slug)
                                .await
                            {
                                Ok(Some(p)) => p,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "run",
                                            json!({ "error": "project not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "project lookup 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "run",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            // Resolve server
                            let servers =
                                match state.db.list_servers_by_tenant(tenant_slug).await {
                                    Ok(s) => s,
                                    Err(e) => {
                                        error!(error = %e, "server lookup 失敗");
                                        channel
                                            .send_response(
                                                msg.id,
                                                "run",
                                                json!({ "error": e.to_string() }),
                                            )
                                            .await?;
                                        continue;
                                    }
                                };

                            let server_model = match servers
                                .iter()
                                .find(|s| s.slug == server_slug)
                            {
                                Some(s) => s,
                                None => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "run",
                                            json!({ "error": format!("server '{}' not found", server_slug) }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            let now = Utc::now();
                            let tenant_id = match tenant.id {
                                Some(id) => id,
                                None => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "run",
                                            json!({ "error": "tenant has no id" }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let project_id = match project.id {
                                Some(id) => id,
                                None => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "run",
                                            json!({ "error": "project has no id" }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let deployment = Deployment {
                                id: None,
                                tenant: tenant_id,
                                project: project_id,
                                stage: stage.into(),
                                server_slug: server_slug.into(),
                                status: "running".into(),
                                command: command.into(),
                                log: None,
                                started_at: Some(now),
                                finished_at: None,
                                created_at: None,
                            };

                            let created = match state.db.create_deployment(&deployment).await {
                                Ok(d) => d,
                                Err(e) => {
                                    error!(error = %e, "deploy.run 記録作成失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "run",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            // Tailscale SSH でリモートコマンド実行
                            // Note: command は CP の信頼されたオペレーターからのみ送信される。
                            // 外部ユーザー入力が直接渡されることはない（CLI/MCP 経由で認証済み）。
                            let ssh_result = fleetflow_cloud::ssh::exec(
                                &server_model.slug,
                                &server_model.ssh_user,
                                command,
                            )
                            .await;

                            let (status, log) = match &ssh_result {
                                Ok(r) if r.success => {
                                    ("success".to_string(), r.stdout.clone())
                                }
                                Ok(r) => (
                                    "failed".to_string(),
                                    format!("exit {}\n{}\n{}", r.exit_code, r.stdout, r.stderr),
                                ),
                                Err(e) => ("failed".to_string(), e.to_string()),
                            };

                            let finished = Utc::now();
                            if let Some(ref id) = created.id {
                                state
                                    .db
                                    .update_deployment_status(
                                        id,
                                        &status,
                                        Some(&log),
                                        Some(finished),
                                    )
                                    .await
                                    .ok();
                            }

                            info!(
                                project = project_slug,
                                stage,
                                server = server_slug,
                                status = status.as_str(),
                                "deploy.run 完了"
                            );

                            channel
                                .send_response(
                                    msg.id,
                                    "run",
                                    json!({
                                        "deployment_id": created.id.map(|id| format!("{id:?}")),
                                        "status": status,
                                        "log": log,
                                    }),
                                )
                                .await?;
                        }
                        "history" => {
                            let tenant_slug =
                                payload["tenant_slug"].as_str().unwrap_or("default");
                            let limit = payload["limit"].as_u64().unwrap_or(20) as usize;

                            match state.db.list_deployments(tenant_slug, limit).await {
                                Ok(deployments) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "history",
                                            json!({ "deployments": deployments }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "deploy.history 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "history",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        method => {
                            info!(method, "deploy: 不明なメソッド");
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
