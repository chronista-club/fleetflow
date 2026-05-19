//! Volume channel handler (Persistence Volume Tier P-2, 2026-04-23)
//!
//! `fleet cp volume <subcommand>` からの Unison Protocol リクエストを処理する。
//! 既存 disk を fleetstage registry に adopt する BYO 経路 + 一覧取得を提供。
//!
//! 詳細設計: fleetstage repo `docs/design/20-persistence-volume-tier.md`

use std::sync::Arc;

use serde_json::json;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::volume_tier;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server
        .register_channel("volume", move |_ctx, stream| {
            let state = state.clone();
            Box::pin(async move {
                let channel = UnisonChannel::new(stream);
                loop {
                    let msg = channel.recv().await?;
                    let payload = msg.payload_as_value()?;

                    match msg.method.as_str() {
                        "list" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();

                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            &json!({ "error": "tenant not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "tenant lookup 失敗 (volume.list)");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let tenant_id = tenant.id.expect("tenant.id");

                            match state.db.list_volumes_by_tenant(&tenant_id).await {
                                Ok(volumes) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            &json!({ "volumes": volumes }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "volume.list 失敗");
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
                        "adopt" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();
                            let server_slug = payload["server_slug"].as_str().unwrap_or_default();
                            let slug = payload["slug"].as_str().unwrap_or_default();
                            let mount = payload["mount"].as_str().unwrap_or_default();
                            let tier = payload["tier"]
                                .as_str()
                                .unwrap_or(volume_tier::LOCAL_VOLUME);

                            // 簡易 validation (B#1 fix: 同 pattern の continue scope バグ修正)
                            let mut empty_field: Option<&str> = None;
                            for (name, value) in [
                                ("tenant_slug", tenant_slug),
                                ("server_slug", server_slug),
                                ("slug", slug),
                                ("mount", mount),
                            ] {
                                if value.is_empty() {
                                    empty_field = Some(name);
                                    break;
                                }
                            }
                            if let Some(name) = empty_field {
                                channel
                                    .send_response(
                                        msg.id,
                                        "adopt",
                                        &json!({ "error": format!("`{}` required", name) }),
                                    )
                                    .await?;
                                continue;
                            }

                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "adopt",
                                            &json!({ "error": "tenant not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "tenant lookup 失敗 (volume.adopt)");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "adopt",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let tenant_id = tenant.id.expect("tenant.id");

                            let srv = match state.db.get_server_by_slug(server_slug).await {
                                Ok(Some(s)) => s,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "adopt",
                                            &json!({
                                                "error": format!("server `{}` not found", server_slug)
                                            }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "server lookup 失敗 (volume.adopt)");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "adopt",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            if srv.tenant != tenant_id {
                                channel
                                    .send_response(
                                        msg.id,
                                        "adopt",
                                        &json!({
                                            "error": "server does not belong to this tenant"
                                        }),
                                    )
                                    .await?;
                                continue;
                            }
                            let server_id = srv.id.expect("server.id");

                            match state
                                .db
                                .adopt_volume(&tenant_id, &server_id, slug, mount, tier)
                                .await
                            {
                                Ok(volume) => {
                                    info!(
                                        slug = %volume.slug,
                                        tier = %volume.tier,
                                        server = %server_slug,
                                        "volume adopted (BYO)"
                                    );
                                    channel
                                        .send_response(
                                            msg.id,
                                            "adopt",
                                            &json!({ "volume": volume }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "volume.adopt 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "adopt",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        other => {
                            channel
                                .send_response(
                                    msg.id,
                                    other,
                                    &json!({ "error": format!("unknown method: {}", other) }),
                                )
                                .await?;
                        }
                    }
                }
            })
        })
        .await;
}
