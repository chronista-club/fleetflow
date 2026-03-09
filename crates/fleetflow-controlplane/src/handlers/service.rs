use std::sync::Arc;

use serde_json::json;
use surrealdb::types::RecordId;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server.register_channel("service", move |_ctx, stream| {
        let state = state.clone();
        Box::pin(async move {
            let channel = UnisonChannel::new(stream);
            loop {
                let msg = channel.recv().await?;
                let payload = msg.payload_as_value()?;

                match msg.method.as_str() {
                    "list" => {
                        let stage_id_str = payload["stage_id"].as_str().unwrap_or_default();
                        let stage_id = RecordId::parse_simple(stage_id_str)
                            .unwrap_or_else(|_| RecordId::new("stage", stage_id_str));

                        match state.db.list_services_by_stage(&stage_id).await {
                            Ok(services) => {
                                channel
                                    .send_response(
                                        msg.id,
                                        "list",
                                        json!({ "services": services }),
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!(error = %e, "service.list 失敗");
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
                    method => {
                        info!(method, "service: 不明なメソッド");
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
