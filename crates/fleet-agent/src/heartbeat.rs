//! ハートビート — 定期的に CP にサーバー状態を報告

use std::sync::Arc;

use serde_json::json;
use tracing::{debug, warn};
use unison::network::client::ProtocolClient;

/// ハートビート送信ループ
pub async fn run_loop(client: &Arc<ProtocolClient>, server_slug: &str, interval_secs: u64) {
    let interval = std::time::Duration::from_secs(interval_secs);

    loop {
        tokio::time::sleep(interval).await;

        match send_heartbeat(client, server_slug).await {
            Ok(()) => debug!(server = server_slug, "ハートビート送信"),
            Err(e) => warn!(error = %e, "ハートビート送信失敗"),
        }
    }
}

async fn send_heartbeat(client: &ProtocolClient, server_slug: &str) -> anyhow::Result<()> {
    let channel = client.open_channel("health").await?;

    channel
        .request(
            "heartbeat",
            json!({
                "server_slug": server_slug,
                "agent_version": env!("CARGO_PKG_VERSION"),
            }),
        )
        .await?;

    channel.close().await.ok();
    Ok(())
}
