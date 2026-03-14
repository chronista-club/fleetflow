use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::{Server, ServerStatusUpdate};
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
                                provision_version: payload["provision_version"]
                                    .as_str()
                                    .map(String::from),
                                tool_versions: payload.get("tool_versions").cloned(),
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
                            let pv = payload["provision_version"].as_str();
                            let tv = payload.get("tool_versions");

                            // バージョン情報があれば一緒に更新
                            let result = if pv.is_some() || tv.is_some() {
                                state
                                    .db
                                    .update_server_versions(server_slug, pv, tv)
                                    .await
                            } else {
                                state.db.update_server_heartbeat(server_slug).await
                            };

                            match result {
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
                        "check-all" => {
                            // Tailscale ステータスを取得し、DB 上のサーバーとマッチング
                            let peers = match fleetflow_cloud::tailscale::get_peers().await {
                                Ok(p) => p,
                                Err(e) => {
                                    error!(error = %e, "tailscale status 取得失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "check-all",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            let servers = match state.db.list_all_servers().await {
                                Ok(s) => s,
                                Err(e) => {
                                    error!(error = %e, "サーバー一覧取得失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "check-all",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            let now = Utc::now();
                            let updates: Vec<ServerStatusUpdate> = servers
                                .iter()
                                .map(|s| {
                                    let peer = peers
                                        .iter()
                                        .find(|p| p.hostname.eq_ignore_ascii_case(&s.slug));
                                    let (status, heartbeat) =
                                        fleetflow_cloud::tailscale::resolve_peer_status(
                                            peer,
                                            s.last_heartbeat_at,
                                            now,
                                        );
                                    ServerStatusUpdate {
                                        slug: s.slug.clone(),
                                        status,
                                        last_heartbeat_at: heartbeat,
                                    }
                                })
                                .collect();

                            match state.db.bulk_update_server_status(&updates).await {
                                Ok(count) => {
                                    let results: Vec<serde_json::Value> = updates
                                        .iter()
                                        .map(|u| {
                                            json!({
                                                "slug": u.slug,
                                                "status": u.status,
                                            })
                                        })
                                        .collect();
                                    channel
                                        .send_response(
                                            msg.id,
                                            "check-all",
                                            json!({ "updated": count, "results": results }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.check-all 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "check-all",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "get" => {
                            let slug = payload["slug"].as_str().unwrap_or_default();

                            match state.db.get_server_by_slug(slug).await {
                                Ok(Some(server)) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            json!({ "server": server }),
                                        )
                                        .await?;
                                }
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            json!({ "error": "server not found" }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.get 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "create" => {
                            // クラウドプロバイダーにサーバーを作成 + DB 登録
                            let provider = match &state.server_provider {
                                Some(p) => p,
                                None => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            json!({ "error": "server provider not configured" }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();
                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
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
                                            "create",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            let request: fleetflow_cloud::CreateServerRequest =
                                match serde_json::from_value(payload["request"].clone()) {
                                    Ok(r) => r,
                                    Err(e) => {
                                        channel
                                            .send_response(
                                                msg.id,
                                                "create",
                                                json!({ "error": format!("invalid request: {}", e) }),
                                            )
                                            .await?;
                                        continue;
                                    }
                                };

                            info!(name = %request.name, "server.create: クラウドにサーバー作成中");

                            // 1. クラウドにサーバーを作成
                            let spec = match provider.create_server(&request).await {
                                Ok(s) => s,
                                Err(e) => {
                                    error!(error = %e, "server.create クラウド作成失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            info!(
                                id = %spec.id,
                                name = %spec.name,
                                ip = ?spec.ip_address,
                                "server.create: クラウドサーバー作成完了"
                            );

                            // 2. DB にサーバーを登録
                            let server_model = Server {
                                id: None,
                                tenant: tenant.id.unwrap(),
                                slug: request.name.clone(),
                                provider: provider.provider_name().into(),
                                plan: Some(format!(
                                    "{}core-{}gb",
                                    request.cpu, request.memory_gb
                                )),
                                ssh_host: spec
                                    .ip_address
                                    .clone()
                                    .unwrap_or_default(),
                                ssh_user: "root".into(),
                                deploy_path: "/opt/fleetflow".into(),
                                status: spec.status.to_string(),
                                provision_version: None,
                                tool_versions: None,
                                last_heartbeat_at: None,
                                created_at: None,
                                updated_at: None,
                            };

                            match state.db.register_server(&server_model).await {
                                Ok(registered) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            json!({
                                                "server": registered,
                                                "cloud": spec,
                                            }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    // クラウドには作成されたが DB 登録に失敗
                                    error!(error = %e, "server.create DB登録失敗（クラウドには作成済み）");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            json!({
                                                "error": e.to_string(),
                                                "cloud": spec,
                                                "warning": "クラウドにはサーバーが作成されています"
                                            }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "delete" => {
                            let slug = payload["slug"].as_str().unwrap_or_default();
                            let with_disks = payload["with_disks"].as_bool().unwrap_or(true);
                            let cloud_id = payload["cloud_id"].as_str();

                            // クラウドからも削除する場合
                            if let Some(cloud_id) = cloud_id {
                                match &state.server_provider {
                                    Some(provider) => {
                                        info!(slug, cloud_id, "server.delete: クラウドサーバー削除中");
                                        if let Err(e) =
                                            provider.delete_server(cloud_id, with_disks).await
                                        {
                                            error!(error = %e, "server.delete クラウド削除失敗");
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "delete",
                                                    json!({ "error": e.to_string() }),
                                                )
                                                .await?;
                                            continue;
                                        }
                                    }
                                    None => {
                                        channel
                                            .send_response(
                                                msg.id,
                                                "delete",
                                                json!({ "error": "server provider not configured (cannot delete from cloud)" }),
                                            )
                                            .await?;
                                        continue;
                                    }
                                }
                            }

                            // DB から削除
                            match state.db.delete_server(slug).await {
                                Ok(()) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "delete",
                                            json!({ "deleted": slug }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.delete DB削除失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "delete",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "power-on" => {
                            let cloud_id = payload["cloud_id"].as_str().unwrap_or_default();

                            match &state.server_provider {
                                Some(provider) => {
                                    match provider.power_on(cloud_id).await {
                                        Ok(()) => {
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "power-on",
                                                    json!({ "ok": true }),
                                                )
                                                .await?;
                                        }
                                        Err(e) => {
                                            error!(error = %e, "server.power-on 失敗");
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "power-on",
                                                    json!({ "error": e.to_string() }),
                                                )
                                                .await?;
                                        }
                                    }
                                }
                                None => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "power-on",
                                            json!({ "error": "server provider not configured" }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "power-off" => {
                            let cloud_id = payload["cloud_id"].as_str().unwrap_or_default();

                            match &state.server_provider {
                                Some(provider) => {
                                    match provider.power_off(cloud_id).await {
                                        Ok(()) => {
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "power-off",
                                                    json!({ "ok": true }),
                                                )
                                                .await?;
                                        }
                                        Err(e) => {
                                            error!(error = %e, "server.power-off 失敗");
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "power-off",
                                                    json!({ "error": e.to_string() }),
                                                )
                                                .await?;
                                        }
                                    }
                                }
                                None => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "power-off",
                                            json!({ "error": "server provider not configured" }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "ping" => {
                            let hostname = payload["hostname"].as_str().unwrap_or_default();

                            match fleetflow_cloud::tailscale::ping(hostname).await {
                                Ok(result) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "ping",
                                            json!({
                                                "hostname": result.hostname,
                                                "reachable": result.reachable,
                                                "latency_ms": result.latency_ms,
                                                "via": result.via,
                                            }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.ping 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "ping",
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
