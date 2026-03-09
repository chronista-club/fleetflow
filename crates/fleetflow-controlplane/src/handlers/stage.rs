use std::sync::Arc;

use serde_json::json;
use surrealdb::types::RecordId;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::Stage;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server.register_channel("stage", move |_ctx, stream| {
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
                                        json!({ "stage": created }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "stage.create 失敗");
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
                    "list" => {
                        let project_id_str = payload["project_id"].as_str().unwrap_or_default();
                        let project_id = RecordId::parse_simple(project_id_str)
                            .unwrap_or_else(|_| RecordId::new("project", project_id_str));

                        match state.db.list_stages_by_project(&project_id).await {
                            Ok(stages) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "list",
                                        json!({ "stages": stages }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "stage.list 失敗");
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
                                        json!({ "stages": stages }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "stage.list_across_projects 失敗");
                                channel
                                    .send_response(
                                        msg.id,
                                        "list_across_projects",
                                        json!({ "error": e.to_string() }),
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
                                json!({ "error": format!("unknown method: {}", method) }),
                            )
                            .await?;
                    }
                }
            }
        })
    });
}
