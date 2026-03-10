use std::sync::Arc;

use serde_json::json;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::Server;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server
        .register_channel("server", move |_ctx, stream| {
            let state = state.clone();
            Box::pin(async move {
                let channel = UnisonChannel::new(stream);
                loop {
                    let msg = channel.recv().await?;
                    let payload = msg.payload_as_value()?;

                    match msg.method.as_str() {
                        "list" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();

                            match state.db.list_servers_by_tenant(tenant_slug).await {
                                Ok(servers) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            json!({ "servers": servers }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.list 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "register" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();

                            // Resolve tenant
                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "register",
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
                                            "register",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            let server_model = Server {
                                id: None,
                                tenant: tenant.id.unwrap(),
                                slug: payload["slug"].as_str().unwrap_or_default().into(),
                                provider: payload["provider"].as_str().unwrap_or_default().into(),
                                plan: payload["plan"].as_str().map(String::from),
                                ssh_host: payload["ssh_host"].as_str().unwrap_or_default().into(),
                                ssh_user: payload["ssh_user"].as_str().unwrap_or("root").into(),
                                deploy_path: payload["deploy_path"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .into(),
                                status: "offline".into(),
                                last_heartbeat_at: None,
                                created_at: None,
                                updated_at: None,
                            };

                            match state.db.register_server(&server_model).await {
                                Ok(created) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "register",
                                            json!({ "server": created }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.register 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "register",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "heartbeat" => {
                            let server_slug = payload["server_slug"].as_str().unwrap_or_default();

                            match state.db.update_server_heartbeat(server_slug).await {
                                Ok(()) => {
                                    channel
                                        .send_response(msg.id, "heartbeat", json!({ "ack": true }))
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.heartbeat 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "heartbeat",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        method => {
                            info!(method, "server: 不明なメソッド");
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
