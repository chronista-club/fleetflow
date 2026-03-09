use std::sync::Arc;

use serde_json::json;
use surrealdb::types::RecordId;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::DnsRecord;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server.register_channel("dns", move |_ctx, stream| {
        let state = state.clone();
        Box::pin(async move {
            let channel = UnisonChannel::new(stream);
            loop {
                let msg = channel.recv().await?;
                let payload = msg.payload_as_value()?;

                match msg.method.as_str() {
                    "create" => {
                        let tenant_slug = payload["tenant_slug"].as_str().unwrap_or("default");
                        let project_slug = payload["project_slug"].as_str();
                        let name = payload["name"].as_str().unwrap_or_default();
                        let record_type = payload["record_type"].as_str().unwrap_or("A");
                        let content = payload["content"].as_str().unwrap_or_default();
                        let zone_id = payload["zone_id"].as_str().map(String::from);
                        let cf_record_id = payload["cf_record_id"].as_str().map(String::from);
                        let proxied = payload["proxied"].as_bool().unwrap_or(false);

                        let tenant_id = RecordId::new("tenant", tenant_slug);
                        let project_id = project_slug.map(|s| RecordId::new("project", s));

                        let record = DnsRecord {
                            id: None,
                            tenant: tenant_id,
                            project: project_id,
                            name: name.into(),
                            record_type: record_type.into(),
                            content: content.into(),
                            zone_id,
                            cf_record_id,
                            proxied,
                            created_at: None,
                            updated_at: None,
                        };

                        match state.db.create_dns_record(&record).await {
                            Ok(created) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "create",
                                        json!({ "dns_record": created }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "dns.create 失敗");
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
                        let tenant_slug = payload["tenant_slug"].as_str().unwrap_or("default");

                        match state.db.list_dns_records_by_tenant(tenant_slug).await {
                            Ok(records) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "list",
                                        json!({ "dns_records": records }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "dns.list 失敗");
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
                    "delete" => {
                        let name = payload["name"].as_str().unwrap_or_default();

                        match state.db.delete_dns_record(name).await {
                            Ok(deleted) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "delete",
                                        json!({ "deleted": deleted }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "dns.delete 失敗");
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
                    method => {
                        info!(method, "dns: 不明なメソッド");
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
