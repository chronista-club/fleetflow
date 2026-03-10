//! バックグラウンド ヘルスチェッカー
//!
//! 定期的に Tailscale ステータスを取得し、DB 上のサーバーステータスを自動更新する。

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use fleetflow_controlplane::server::AppState;
use tracing::{error, info, warn};

use fleetflow_cloud::tailscale;
use fleetflow_controlplane::model::ServerStatusUpdate;

/// ヘルスチェッカーをバックグラウンドで起動
///
/// `interval` 秒ごとに Tailscale ステータスを取得し、DB を更新する。
pub fn spawn(state: Arc<AppState>, interval_secs: u64) -> tokio::task::JoinHandle<()> {
    let interval = Duration::from_secs(interval_secs);

    tokio::spawn(async move {
        info!(interval_secs, "ヘルスチェッカー起動");

        loop {
            tokio::time::sleep(interval).await;

            if let Err(e) = run_check(&state).await {
                error!(error = %e, "ヘルスチェック失敗");
            }
        }
    })
}

async fn run_check(state: &AppState) -> anyhow::Result<()> {
    let peers = match tailscale::get_peers().await {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "Tailscale ステータス取得失敗（スキップ）");
            return Ok(());
        }
    };

    let servers = state.db.list_all_servers().await?;
    if servers.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    let updates: Vec<ServerStatusUpdate> = servers
        .iter()
        .map(|s| {
            let peer = peers
                .iter()
                .find(|p| p.hostname.eq_ignore_ascii_case(&s.slug));
            let (status, heartbeat) =
                tailscale::resolve_peer_status(peer, s.last_heartbeat_at, now);
            ServerStatusUpdate {
                slug: s.slug.clone(),
                status,
                last_heartbeat_at: heartbeat,
            }
        })
        .collect();

    let count = state.db.bulk_update_server_status(&updates).await?;
    info!(updated = count, total = servers.len(), "ヘルスチェック完了");

    Ok(())
}
