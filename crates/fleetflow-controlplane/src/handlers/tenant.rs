use std::sync::Arc;

use serde_json::json;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::Tenant;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server.register_channel("tenant", move |_ctx, stream| {
        let state = state.clone();
        Box::pin(async move {
            let channel = UnisonChannel::new(stream);
            loop {
                let msg = channel.recv().await?;
                match msg.method.as_str() {
                    "get" => {
                        let payload = msg.payload_as_value()?;
                        let slug = payload["slug"].as_str().unwrap_or_default();

                        match state.db.get_tenant_by_slug(slug).await {
                            Ok(Some(tenant)) => {
                                channel
                                    .send_response(msg.id, "get", json!({ "tenant": tenant }))
                                    .await?;
                            }
                            Ok(None) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "get",
                                        json!({ "error": "tenant not found" }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "tenant.get 失敗");
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
                        let payload = msg.payload_as_value()?;
                        let slug = payload["slug"].as_str().unwrap_or_default();
                        let name = payload["name"].as_str().unwrap_or(slug);
                        let plan = payload["plan"].as_str().unwrap_or("self-hosted");
                        let auth0_org_id = payload["auth0_org_id"].as_str().map(String::from);

                        let tenant = Tenant {
                            id: None,
                            slug: slug.into(),
                            name: name.into(),
                            auth0_org_id,
                            plan: plan.into(),
                            created_at: None,
                            updated_at: None,
                        };

                        match state.db.create_tenant(&tenant).await {
                            Ok(created) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "create",
                                        json!({ "tenant": created }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "tenant.create 失敗");
                                channel
                                    .send_response(
                                        msg.id,
                                        "create",
                                        json!({ "error": e.to_string() }),
                                    )
                                    .await?;
                            }
                        }
                    }
                    "list" => match state.db.list_tenants().await {
                        Ok(tenants) => {
                            channel
                                .send_response(msg.id, "list", json!({ "tenants": tenants }))
                                .await?;
                        }
                        Err(e) => {
                            error!(error = %e, "tenant.list 失敗");
                            channel
                                .send_response(
                                    msg.id,
                                    "list",
                                    json!({ "error": e.to_string() }),
                                )
                                .await?;
                        }
                    },
                    method => {
                        info!(method, "tenant: 不明なメソッド");
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
    }).await;
}
