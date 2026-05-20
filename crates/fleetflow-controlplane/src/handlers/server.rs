use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::{Alert, ObservedContainer, Server, ServerStatusUpdate};
use crate::server::AppState;

/// `inventory_report` payload の `containers` 配列を `Vec<ObservedContainer>` に
/// 変換する（純粋関数 — テスト可能）。
///
/// agent (fleet-agent monitor) が送る wire 形式:
/// `{ server_slug, containers: [{ runtime, container_id, container_name,
///    status, health?, image?, project?, stage?, service?, started_at? }] }`
fn parse_inventory_payload(payload: &serde_json::Value) -> Vec<ObservedContainer> {
    let server_slug = payload["server_slug"].as_str().unwrap_or_default();
    let now = Utc::now();
    payload["containers"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    // container_id / container_name は必須。欠落要素はスキップ。
                    let container_id = c["container_id"].as_str()?.to_string();
                    let container_name = c["container_name"].as_str()?.to_string();
                    let opt_str =
                        |key: &str| -> Option<String> { c[key].as_str().map(str::to_string) };
                    Some(ObservedContainer {
                        id: None,
                        server_slug: server_slug.to_string(),
                        runtime: c["runtime"].as_str().unwrap_or("unknown").to_string(),
                        container_id,
                        container_name,
                        status: c["status"].as_str().unwrap_or("unknown").to_string(),
                        health: opt_str("health"),
                        image: opt_str("image"),
                        project: opt_str("project"),
                        stage: opt_str("stage"),
                        service: opt_str("service"),
                        started_at: c["started_at"]
                            .as_str()
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&Utc)),
                        last_seen_at: Some(now),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

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
                                            &json!({ "servers": servers }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.list 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            &json!({ "error": e.to_string() }),
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
                                            &json!({ "error": "tenant not found" }),
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
                                            &json!({ "error": e.to_string() }),
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
                                // FSC-26 Phase B-1: register 経由では未設定、後続 API で付与
                                labels: None,
                                capacity: None,
                                allocated: None,
                                scheduling: None,
                                // FSC-26 Phase B-2: migration で worker_pool:default に紐付け
                                pool_id: None,
                                // single-table lifecycle model
                                desired_state: Some("running".into()),
                                purpose: None,
                                owner: None,
                                sakura: None,
                                tailscale: None,
                                dns: None,
                                lifecycle: None,
                                created_at: None,
                                updated_at: None,
                                deleted_at: None,
                            };

                            match state.db.register_server(&server_model).await {
                                Ok(created) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "register",
                                            &json!({ "server": created }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.register 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "register",
                                            &json!({ "error": e.to_string() }),
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
                                        .send_response(msg.id, "heartbeat", &json!({ "ack": true }))
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.heartbeat 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "heartbeat",
                                            &json!({ "error": e.to_string() }),
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
                                            &json!({ "error": e.to_string() }),
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
                                            &json!({ "error": e.to_string() }),
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
                                            &json!({ "updated": count, "results": results }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.check-all 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "check-all",
                                            &json!({ "error": e.to_string() }),
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
                                            &json!({ "server": server }),
                                        )
                                        .await?;
                                }
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            &json!({ "error": "server not found" }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.get 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            &json!({ "error": e.to_string() }),
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
                                            &json!({ "error": "server provider not configured" }),
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
                                            &json!({ "error": "tenant not found" }),
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
                                            &json!({ "error": e.to_string() }),
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
                                                &json!({ "error": format!("invalid request: {}", e) }),
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
                                            &json!({ "error": e.to_string() }),
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
                            let ssh_host = match &spec.ip_address {
                                Some(ip) if !ip.is_empty() => ip.clone(),
                                _ => {
                                    error!("server.create: IP アドレスが取得できませんでした");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({
                                                "error": "server created but no IP address assigned",
                                                "cloud": spec,
                                            }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            let server_model = Server {
                                id: None,
                                tenant: tenant.id.unwrap(),
                                slug: request.name.clone(),
                                provider: provider.provider_name().into(),
                                plan: Some(format!(
                                    "{}core-{}gb",
                                    request.cpu, request.memory_gb
                                )),
                                ssh_host,
                                ssh_user: "root".into(),
                                deploy_path: "/opt/fleetflow".into(),
                                status: spec.status.to_string(),
                                provision_version: None,
                                tool_versions: None,
                                last_heartbeat_at: None,
                                // FSC-26 Phase B-1: create 経由では未設定、後続 API で付与
                                labels: None,
                                capacity: None,
                                allocated: None,
                                scheduling: None,
                                // FSC-26 Phase B-2: migration で worker_pool:default に紐付け
                                pool_id: None,
                                // single-table lifecycle model
                                desired_state: Some("running".into()),
                                purpose: None,
                                owner: None,
                                sakura: None,
                                tailscale: None,
                                dns: None,
                                lifecycle: None,
                                created_at: None,
                                updated_at: None,
                                deleted_at: None,
                            };

                            match state.db.register_server(&server_model).await {
                                Ok(registered) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({
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
                                            &json!({
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
                            // C1: disk 削除は明示的 opt-in (`feedback_disk_deletion_confirm.md`)。
                            // field omit / typo で disk silent destroy を回避するため default false。
                            let with_disks = payload["with_disks"].as_bool().unwrap_or(false);
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
                                                    &json!({ "error": e.to_string() }),
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
                                                &json!({ "error": "server provider not configured (cannot delete from cloud)" }),
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
                                            &json!({ "deleted": slug }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "server.delete DB削除失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "delete",
                                            &json!({ "error": e.to_string() }),
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
                                                    &json!({ "ok": true }),
                                                )
                                                .await?;
                                        }
                                        Err(e) => {
                                            error!(error = %e, "server.power-on 失敗");
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "power-on",
                                                    &json!({ "error": e.to_string() }),
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
                                            &json!({ "error": "server provider not configured" }),
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
                                                    &json!({ "ok": true }),
                                                )
                                                .await?;
                                        }
                                        Err(e) => {
                                            error!(error = %e, "server.power-off 失敗");
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "power-off",
                                                    &json!({ "error": e.to_string() }),
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
                                            &json!({ "error": "server provider not configured" }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "alert" => {
                            let server_slug =
                                payload["server_slug"].as_str().unwrap_or_default();
                            let container_name =
                                payload["container_name"].as_str().unwrap_or_default();
                            let alert_type =
                                payload["alert_type"].as_str().unwrap_or_default();
                            let severity =
                                payload["severity"].as_str().unwrap_or("warning");
                            let message =
                                payload["message"].as_str().unwrap_or_default();

                            // server_slug → テナント解決
                            let server = match state.db.get_server_by_slug(server_slug).await {
                                Ok(Some(s)) => s,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "alert",
                                            &json!({ "error": "server not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "server lookup 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "alert",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            let alert = Alert {
                                id: None,
                                tenant: server.tenant.clone(),
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
                                Ok(_) => {
                                    info!(
                                        server = server_slug,
                                        container = container_name,
                                        alert_type,
                                        "アラート記録完了"
                                    );
                                    channel
                                        .send_response(
                                            msg.id,
                                            "alert",
                                            &json!({ "ack": true }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "アラート記録失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "alert",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "alert_resolve" => {
                            let server_slug =
                                payload["server_slug"].as_str().unwrap_or_default();
                            let container_name =
                                payload["container_name"].as_str().unwrap_or_default();

                            match state.db.resolve_alerts(server_slug, container_name).await {
                                Ok(()) => {
                                    info!(
                                        server = server_slug,
                                        container = container_name,
                                        "アラート解決完了"
                                    );
                                    channel
                                        .send_response(
                                            msg.id,
                                            "alert_resolve",
                                            &json!({ "ack": true }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "アラート解決失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "alert_resolve",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "inventory_report" => {
                            // #185: agent monitor が全 runtime から観測した
                            // container snapshot を server 単位で全置換する。
                            let server_slug =
                                payload["server_slug"].as_str().unwrap_or_default();
                            let containers = parse_inventory_payload(&payload);

                            match state
                                .db
                                .replace_observed_containers(server_slug, &containers)
                                .await
                            {
                                Ok(()) => {
                                    info!(
                                        server = server_slug,
                                        count = containers.len(),
                                        "inventory_report 記録完了"
                                    );
                                    channel
                                        .send_response(
                                            msg.id,
                                            "inventory_report",
                                            &json!({ "ack": true, "count": containers.len() }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "inventory_report 記録失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "inventory_report",
                                            &json!({ "error": e.to_string() }),
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
                                            &json!({
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
                                            &json!({ "error": e.to_string() }),
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
                                    &json!({ "error": format!("unknown method: {}", method) }),
                                )
                                .await?;
                        }
                    }
                }
            })
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::parse_inventory_payload;
    use serde_json::json;

    /// agent の wire 形式を ObservedContainer に変換できる。
    #[test]
    fn parse_inventory_payload_well_formed() {
        let payload = json!({
            "server_slug": "worker-01",
            "containers": [
                {
                    "runtime": "podman-rootless-1000",
                    "container_id": "abc123",
                    "container_name": "fleetstage-hq-api",
                    "status": "running",
                    "image": "ghcr.io/x/hq-api:latest",
                    "project": "fleetstage",
                    "stage": "live",
                    "service": "hq-api",
                },
            ],
        });
        let containers = parse_inventory_payload(&payload);
        assert_eq!(containers.len(), 1);
        let c = &containers[0];
        assert_eq!(c.server_slug, "worker-01");
        assert_eq!(c.runtime, "podman-rootless-1000");
        assert_eq!(c.container_name, "fleetstage-hq-api");
        assert_eq!(c.project.as_deref(), Some("fleetstage"));
        assert_eq!(c.stage.as_deref(), Some("live"));
        assert!(c.last_seen_at.is_some(), "last_seen_at は CP 側で付与");
        assert!(c.id.is_none());
    }

    /// container_id / container_name 欠落要素はスキップ、optional 欠落は None。
    #[test]
    fn parse_inventory_payload_skips_invalid_and_defaults_optionals() {
        let payload = json!({
            "server_slug": "worker-01",
            "containers": [
                { "container_id": "ok", "container_name": "valid" },
                { "container_id": "no-name" },          // container_name 欠落 → skip
                { "container_name": "no-id" },          // container_id 欠落 → skip
            ],
        });
        let containers = parse_inventory_payload(&payload);
        assert_eq!(containers.len(), 1, "必須欠落要素はスキップされる");
        let c = &containers[0];
        assert_eq!(c.container_name, "valid");
        assert_eq!(c.runtime, "unknown", "runtime 欠落時は unknown");
        assert_eq!(c.status, "unknown", "status 欠落時は unknown");
        assert!(c.project.is_none());
        assert!(c.image.is_none());
    }

    /// containers キー欠落・空でも panic せず空 Vec。
    #[test]
    fn parse_inventory_payload_empty() {
        assert!(parse_inventory_payload(&json!({ "server_slug": "w" })).is_empty());
        assert!(
            parse_inventory_payload(&json!({ "server_slug": "w", "containers": [] })).is_empty()
        );
    }
}
