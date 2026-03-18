//! Fleet Agent — サーバーエージェント
//!
//! 各 worker サーバーに1つ配置し、fleetflowd（Control Plane）との
//! Unison Protocol 通信でデプロイ実行・コンテナ監視を行う。

use anyhow::Result;
use clap::Parser;
use fleet_agent::agent;
use tracing::info;

#[derive(Parser)]
#[command(name = "fleet-agent", about = "FleetFlow server agent")]
struct Cli {
    /// Control Plane の Unison Protocol エンドポイント
    #[arg(long, env = "FLEET_AGENT_CP_ENDPOINT", default_value = "[::1]:4510")]
    cp_endpoint: String,

    /// このサーバーの slug（CP に登録された名前）
    #[arg(long, env = "FLEET_AGENT_SERVER_SLUG")]
    server_slug: String,

    /// ハートビート間隔（秒）
    #[arg(long, env = "FLEET_AGENT_HEARTBEAT_INTERVAL", default_value = "30")]
    heartbeat_interval: u64,

    /// デプロイ許可ベースディレクトリ
    #[arg(long, env = "FLEET_AGENT_DEPLOY_BASE", default_value = "/opt/apps")]
    deploy_base: String,

    /// モニター間隔（秒）
    #[arg(long, env = "FLEET_AGENT_MONITOR_INTERVAL", default_value = "30")]
    monitor_interval: u64,

    /// リスタート閾値（この回数を超えたらアラート）
    #[arg(long, env = "FLEET_AGENT_RESTART_THRESHOLD", default_value = "3")]
    restart_threshold: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fleet_agent=info".into()),
        )
        .init();

    let cli = Cli::parse();

    info!(
        server = %cli.server_slug,
        endpoint = %cli.cp_endpoint,
        deploy_base = %cli.deploy_base,
        "Fleet Agent 起動"
    );

    agent::run(agent::AgentConfig {
        cp_endpoint: cli.cp_endpoint,
        server_slug: cli.server_slug,
        heartbeat_interval_secs: cli.heartbeat_interval,
        deploy_base: cli.deploy_base,
        monitor_interval_secs: cli.monitor_interval,
        restart_threshold: cli.restart_threshold,
    })
    .await
}
