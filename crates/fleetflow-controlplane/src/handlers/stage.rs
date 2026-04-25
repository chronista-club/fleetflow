use std::sync::Arc;

use serde_json::json;
use surrealdb::types::RecordId;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::{AdoptServiceSpec, AdoptStageRequest, Stage};
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server
        .register_channel("stage", move |_ctx, stream| {
            let state = state.clone();
            Box::pin(async move {
                let channel = UnisonChannel::new(stream);
                loop {
                    let msg = channel.recv().await?;
                    let payload = msg.payload_as_value()?;

                    match msg.method.as_str() {
                        "create" => {
                            let project_id = payload["project_id"].as_str().unwrap_or_default();
                            let slug = payload["slug"].as_str().unwrap_or_default();
                            let description = payload["description"].as_str().map(String::from);

                            let stage = Stage {
                                id: None,
                                project: RecordId::new("project", project_id),
                                slug: slug.into(),
                                description,
                                server: None,
                                created_at: None,
                                updated_at: None,
                            };

                            match state.db.create_stage(&stage).await {
                                Ok(created) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({ "stage": created }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "stage.create 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "list" => {
                            let project_id_str = payload["project_id"].as_str().unwrap_or_default();
                            let project_id = RecordId::parse_simple(project_id_str)
                                .unwrap_or_else(|_| RecordId::new("project", project_id_str));

                            match state.db.list_stages_by_project(&project_id).await {
                                Ok(stages) => {
                                    channel
                                        .send_response(msg.id, "list", &json!({ "stages": stages }))
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "stage.list 失敗");
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
                        "list_across_projects" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();
                            let stage_slug = payload["stage_slug"].as_str().unwrap_or_default();

                            match state
                                .db
                                .list_stages_across_projects(tenant_slug, stage_slug)
                                .await
                            {
                                Ok(stages) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list_across_projects",
                                            &json!({ "stages": stages }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "stage.list_across_projects 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list_across_projects",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "adopt" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();
                            let server_slug = payload["server_slug"].as_str().unwrap_or_default();
                            let project_slug = payload["project_slug"].as_str().unwrap_or_default();
                            let project_name = payload["project_name"].as_str();
                            let stage_slug = payload["stage_slug"].as_str().unwrap_or_default();
                            let description = payload["description"].as_str();

                            // 必須フィールド validation
                            for (name, value) in [
                                ("tenant_slug", tenant_slug),
                                ("server_slug", server_slug),
                                ("project_slug", project_slug),
                                ("stage_slug", stage_slug),
                            ] {
                                if value.is_empty() {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "adopt",
                                            &json!({ "error": format!("`{}` required", name) }),
                                        )
                                        .await?;
                                    continue;
                                }
                            }

                            // services 配列をパース
                            let services: Vec<AdoptServiceSpec> = match payload["services"]
                                .as_array()
                            {
                                Some(arr) if !arr.is_empty() => arr
                                    .iter()
                                    .filter_map(|s| {
                                        Some(AdoptServiceSpec {
                                            slug: s["slug"].as_str()?.to_string(),
                                            image: s["image"].as_str()?.to_string(),
                                        })
                                    })
                                    .collect(),
                                _ => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "adopt",
                                            &json!({ "error": "`services` must be a non-empty array of { slug, image }" }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            // tenant 解決
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
                                    error!(error = %e, "tenant lookup 失敗 (stage.adopt)");
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

                            // server 解決 (tenant 配下確認)
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
                                    error!(error = %e, "server lookup 失敗 (stage.adopt)");
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
                                        &json!({ "error": "server does not belong to this tenant" }),
                                    )
                                    .await?;
                                continue;
                            }
                            let server_id = srv.id.expect("server.id");

                            match state
                                .db
                                .adopt_stage(&AdoptStageRequest {
                                    tenant_id: &tenant_id,
                                    server_id: &server_id,
                                    project_slug,
                                    project_name,
                                    stage_slug,
                                    description,
                                    services: &services,
                                })
                                .await
                            {
                                Ok(outcome) => {
                                    info!(
                                        project = %project_slug,
                                        stage = %stage_slug,
                                        server = %server_slug,
                                        service_count = outcome.services.len(),
                                        "stage adopted"
                                    );
                                    channel
                                        .send_response(
                                            msg.id,
                                            "adopt",
                                            &json!({ "outcome": outcome }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "stage.adopt 失敗");
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
                        method => {
                            info!(method, "stage: 不明なメソッド");
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
