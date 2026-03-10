mod config;
mod daemon;
mod health;
mod web;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use colored::Colorize;
use tracing::info;

use config::DaemonConfig;
use daemon::DaemonStatus;

#[derive(Parser)]
#[command(name = "fleetflowd")]
#[command(about = "FleetFlow Control Plane デーモン")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 設定ファイルのパス
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// ログレベル（debug, info, warn, error）
    #[arg(long)]
    log_level: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// デーモンを停止
    Stop,
    /// デーモンの状態を表示
    Status,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Rustls CryptoProvider（Unison/QUIC に必要）
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = Cli::parse();

    // 設定ファイルの読み込み
    let cfg = if let Some(config_path) = config::find_config_file(cli.config.as_deref()) {
        info!(path = %config_path.display(), "設定ファイル読み込み");
        config::load_config(&config_path)?
    } else {
        DaemonConfig::default()
    };

    // ログレベルの決定（CLI引数 > 設定ファイル）
    let log_level = cli.log_level.as_deref().unwrap_or(&cfg.log_level);

    // サブコマンド処理
    match cli.command {
        Some(Commands::Stop) => {
            return handle_stop(&cfg);
        }
        Some(Commands::Status) => {
            return handle_status(&cfg);
        }
        None => {
            // フォアグラウンド起動
        }
    }

    // ログ初期化
    if let Some(ref log_file) = cfg.log_file {
        if let Some(parent) = log_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;

        tracing_subscriber::fmt()
            .with_writer(file)
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(log_level.parse().unwrap_or(tracing::Level::INFO.into())),
            )
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(log_level.parse().unwrap_or(tracing::Level::INFO.into())),
            )
            .init();
    }

    // 既存プロセスの確認
    match daemon::check_status(&cfg.pid_file)? {
        DaemonStatus::Running(pid) => {
            eprintln!(
                "{} fleetflowd は既に起動中です (PID: {})",
                "Error:".red().bold(),
                pid
            );
            std::process::exit(1);
        }
        DaemonStatus::Stale(_) => {
            daemon::remove_pid_file(&cfg.pid_file);
        }
        DaemonStatus::Stopped => {}
    }

    // PID ファイル書き込み
    daemon::write_pid_file(&cfg.pid_file)?;

    println!("{}", "FleetFlow Control Plane".bold());
    println!("  Listen: {}", cfg.server.listen_addr.cyan());
    println!("  DB:     {}", cfg.server.db.endpoint.cyan());
    println!("  PID:    {}", std::process::id());
    println!();

    // Control Plane サーバー起動
    let web_addr = cfg.web_addr.clone();
    let (handle, state) = fleetflow_controlplane::server::start(cfg.server).await?;

    // WebUI Dashboard 起動
    let web_handle = web::start(state.clone(), &web_addr).await?;
    println!("  Web:    {}", format!("http://{}", web_addr).cyan());

    // バックグラウンド ヘルスチェッカー起動
    let _health_handle = if cfg.health_check_interval > 0 {
        println!(
            "  Health: {} 秒間隔",
            cfg.health_check_interval.to_string().cyan()
        );
        Some(health::spawn(state, cfg.health_check_interval))
    } else {
        println!("  Health: {}", "無効".dimmed());
        None
    };

    println!("{}", "Control Plane 起動完了。Ctrl+C で停止。".green());

    // シグナル待機
    tokio::signal::ctrl_c().await?;

    println!();
    info!("シャットダウン開始");

    // クリーンアップ
    drop(web_handle);
    drop(handle);
    daemon::remove_pid_file(&cfg.pid_file);

    info!("シャットダウン完了");
    Ok(())
}

fn handle_stop(cfg: &DaemonConfig) -> anyhow::Result<()> {
    match daemon::check_status(&cfg.pid_file)? {
        DaemonStatus::Running(pid) => {
            println!("fleetflowd (PID: {}) を停止中...", pid);
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            // 停止待ち（最大5秒）
            for _ in 0..50 {
                if !daemon::is_process_alive(pid) {
                    daemon::remove_pid_file(&cfg.pid_file);
                    println!("{}", "停止しました。".green());
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            eprintln!("{}", "タイムアウト: プロセスが停止しませんでした。".red());
            std::process::exit(1);
        }
        DaemonStatus::Stale(pid) => {
            println!("stale PID ファイルを削除 (PID: {} は既に停止済み)", pid);
            daemon::remove_pid_file(&cfg.pid_file);
            Ok(())
        }
        DaemonStatus::Stopped => {
            println!("{}", "fleetflowd は起動していません。".yellow());
            Ok(())
        }
    }
}

fn handle_status(cfg: &DaemonConfig) -> anyhow::Result<()> {
    match daemon::check_status(&cfg.pid_file)? {
        DaemonStatus::Running(pid) => {
            println!("{}", "fleetflowd: running".green().bold());
            println!("  PID:      {}", pid);
            println!("  PID file: {}", cfg.pid_file.display());
        }
        DaemonStatus::Stale(pid) => {
            println!("{}", "fleetflowd: stale".yellow().bold());
            println!(
                "  PID file が存在しますが、プロセス (PID: {}) は停止しています。",
                pid
            );
            println!("  `fleetflowd stop` で PID ファイルを削除してください。");
        }
        DaemonStatus::Stopped => {
            println!("{}", "fleetflowd: stopped".red().bold());
            println!("  `fleetflowd` で起動してください。");
        }
    }
    Ok(())
}
