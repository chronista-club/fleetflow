use std::sync::Arc;

use serde_json::json;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::Project;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server
        .register_channel("project", move |_ctx, stream| {
            let state = state.clone();
            Box::pin(async move {
                let channel = UnisonChannel::new(stream);
                loop {
                    let msg = channel.recv().await?;
                    let payload = msg.payload_as_value()?;

                    match msg.method.as_str() {
                        "create" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();
                            let slug = payload["slug"].as_str().unwrap_or_default();
                            let name = payload["name"].as_str().unwrap_or_default();
                            let description = payload["description"].as_str().map(String::from);

                            // Resolve tenant
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

                            // FSC-33: 重複作成を SDK の "Connection uninitialised" 翻訳バグに
                            // 飲まれる前に明示的に検出して 409-equivalent を返す
                            if let Ok(Some(_)) =
                                state.db.get_project_by_slug(tenant_slug, slug).await
                            {
                                channel
                                    .send_response(
                                        msg.id,
                                        "create",
                                        &json!({
                                            "error": format!(
                                                "project '{}' は tenant '{}' に既に存在します",
                                                slug, tenant_slug
                                            )
                                        }),
                                    )
                                    .await?;
                                continue;
                            }

                            let project = Project {
                                id: None,
                                tenant: tenant.id.unwrap(),
                                slug: slug.into(),
                                name: name.into(),
                                description,
                                repository_url: payload["repository_url"]
                                    .as_str()
                                    .map(String::from),
                                created_at: None,
                                updated_at: None,
                                deleted_at: None,
                            };

                            match state.db.create_project(&project).await {
                                Ok(created) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({ "project": created }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "project.create 失敗");
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
                        "get" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();
                            let slug = payload["slug"].as_str().unwrap_or_default();

                            match state.db.get_project_by_slug(tenant_slug, slug).await {
                                Ok(Some(project)) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            &json!({ "project": project }),
                                        )
                                        .await?;
                                }
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            &json!({ "error": "project not found" }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "project.get 失敗");
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
                        "list" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();

                            match state.db.list_projects_by_tenant(tenant_slug).await {
                                Ok(projects) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            &json!({ "projects": projects }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "project.list 失敗");
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
                        "delete" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();
                            let slug = payload["slug"].as_str().unwrap_or_default();

                            match state.db.delete_project(tenant_slug, slug).await {
                                Ok(_) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "delete",
                                            &json!({ "deleted": true }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "project.delete 失敗");
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
                        method => {
                            info!(method, "project: 不明なメソッド");
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
