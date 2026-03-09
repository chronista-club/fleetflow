use std::sync::Arc;

use serde_json::json;
use surrealdb::types::RecordId;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::CostEntry;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server.register_channel("cost", move |_ctx, stream| {
        let state = state.clone();
        Box::pin(async move {
            let channel = UnisonChannel::new(stream);
            loop {
                let msg = channel.recv().await?;
                let payload = msg.payload_as_value()?;

                match msg.method.as_str() {
                    "record" => {
                        let tenant_slug = payload["tenant_slug"].as_str().unwrap_or("default");
                        let project_slug = payload["project_slug"].as_str();
                        let stage = payload["stage"].as_str().map(String::from);
                        let provider = payload["provider"].as_str().unwrap_or_default();
                        let description = payload["description"].as_str().unwrap_or_default();
                        let amount_jpy = payload["amount_jpy"].as_i64().unwrap_or(0);
                        let month = payload["month"].as_str().unwrap_or_default();

                        let tenant_id = RecordId::new("tenant", tenant_slug);
                        let project_id = project_slug.map(|s| RecordId::new("project", s));

                        let entry = CostEntry {
                            id: None,
                            tenant: tenant_id,
                            project: project_id,
                            stage,
                            provider: provider.into(),
                            description: description.into(),
                            amount_jpy,
                            month: month.into(),
                            created_at: None,
                        };

                        match state.db.create_cost_entry(&entry).await {
                            Ok(created) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "record",
                                        json!({ "cost_entry": created }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "cost.record 失敗");
                                channel
                                    .send_response(
                                        msg.id,
                                        "record",
                                        json!({ "error": e.to_string() }),
                                    )
                                    .await?;
                            }
                        }
                    }
                    "list" => {
                        let tenant_slug = payload["tenant_slug"].as_str().unwrap_or("default");
                        let month = payload["month"].as_str().unwrap_or_default();

                        match state.db.list_costs_by_month(tenant_slug, month).await {
                            Ok(entries) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "list",
                                        json!({ "entries": entries }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "cost.list 失敗");
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
                    "summary" => {
                        let tenant_slug = payload["tenant_slug"].as_str().unwrap_or("default");
                        let month = payload["month"].as_str().unwrap_or_default();

                        match state
                            .db
                            .summarize_costs_by_month(tenant_slug, month)
                            .await
                        {
                            Ok(summaries) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "summary",
                                        json!({ "summaries": summaries }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "cost.summary 失敗");
                                channel
                                    .send_response(
                                        msg.id,
                                        "summary",
                                        json!({ "error": e.to_string() }),
                                    )
                                    .await?;
                            }
                        }
                    }
                    method => {
                        info!(method, "cost: 不明なメソッド");
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
