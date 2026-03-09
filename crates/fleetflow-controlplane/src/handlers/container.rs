use std::sync::Arc;

use serde_json::json;
use tracing::info;
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server.register_channel("container", move |_ctx, stream| {
        let _state = state.clone();
        Box::pin(async move {
            let channel = UnisonChannel::new(stream);
            loop {
                let msg = channel.recv().await?;

                match msg.method.as_str() {
                    "start" | "stop" | "restart" => {
                        // TODO: fleetflow-container crate と連携してコンテナ操作
                        channel
                            .send_response(
                                msg.id,
                                &msg.method,
                                json!({ "error": "not implemented yet" }),
                            )
                            .await?;
                    }
                    "logs" => {
                        // TODO: Event push でログストリーミング
                        channel
                            .send_response(
                                msg.id,
                                "logs",
                                json!({ "error": "not implemented yet" }),
                            )
                            .await?;
                    }
                    "exec" => {
                        // TODO: Raw bytes で stdin/stdout
                        channel
                            .send_response(
                                msg.id,
                                "exec",
                                json!({ "error": "not implemented yet" }),
                            )
                            .await?;
                    }
                    method => {
                        info!(method, "container: 不明なメソッド");
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
