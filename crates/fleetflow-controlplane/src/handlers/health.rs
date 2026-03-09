use std::sync::Arc;

use serde_json::json;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server.register_channel("health", move |_ctx, stream| {
        let state = state.clone();
        Box::pin(async move {
            let channel = UnisonChannel::new(stream);
            loop {
                let msg = channel.recv().await?;
                let payload = msg.payload_as_value()?;

                match msg.method.as_str() {
                    "overview" => {
                        let tenant_slug = payload["tenant_slug"].as_str().unwrap_or_default();

                        // Collect health data from all projects/stages/services
                        let projects = state.db.list_projects_by_tenant(tenant_slug).await;
                        let servers = state.db.list_servers_by_tenant(tenant_slug).await;

                        match (projects, servers) {
                            (Ok(projects), Ok(servers)) => {
                                let online_servers =
                                    servers.iter().filter(|s| s.status == "online").count();
                                channel
                                    .send_response(
                                        msg.id,
                                        "overview",
                                        json!({
                                            "tenant_slug": tenant_slug,
                                            "project_count": projects.len(),
                                            "server_count": servers.len(),
                                            "servers_online": online_servers,
                                        }),
                                    )
                                    .await?;
                            }
                            (Err(e), _) | (_, Err(e)) => {
                                error!(error = %e, "health.overview 失敗");
                                channel
                                    .send_response(
                                        msg.id,
                                        "overview",
                                        json!({ "error": e.to_string() }),
                                    )
                                    .await?;
                            }
                        }
                    }
                    method => {
                        info!(method, "health: 不明なメソッド");
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
