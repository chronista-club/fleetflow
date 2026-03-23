mod build;
mod commands;
mod docker;
mod self_update;
mod tui;
mod utils;

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fleet", version)]
#[command(about = "伝える。動く。環境構築は、対話になった。", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 詳細な出力を表示
    #[arg(short, long, global = true, conflicts_with = "quiet")]
    verbose: bool,

    /// エラー以外の出力を抑制
    #[arg(short, long, global = true)]
    quiet: bool,
}

// ─────────────────────────────────────────────
// Top-level commands: Daily(6) + Ship(2) + Util(3) + CP(1)
// ─────────────────────────────────────────────

#[derive(Subcommand)]
enum Commands {
    // ── Daily ──────────────────────────────────

    /// ステージを起動
    Up {
        /// ステージ名 (local, dev, stg, prod)
        stage: Option<String>,
        /// ステージ名 (-s/--stage フラグ、FLEET_STAGE 環境変数)
        #[arg(
            short = 's',
            long = "stage",
            env = "FLEET_STAGE",
            conflicts_with = "stage",
            hide = true
        )]
        stage_flag: Option<String>,
        /// 起動前に最新イメージをpullする
        #[arg(short, long)]
        pull: bool,
        /// 実行せずに実行計画のみ表示
        #[arg(long)]
        dry_run: bool,
    },
    /// ステージを停止
    Down {
        /// ステージ名 (local, dev, stg, prod)
        stage: Option<String>,
        /// ステージ名 (-s/--stage フラグ、FLEET_STAGE 環境変数)
        #[arg(
            short = 's',
            long = "stage",
            env = "FLEET_STAGE",
            conflicts_with = "stage",
            hide = true
        )]
        stage_flag: Option<String>,
        /// コンテナを削除する（デフォルトは停止のみ）
        #[arg(short, long)]
        remove: bool,
    },
    /// サービスまたはステージ全体を再起動
    Restart {
        /// ステージ名 (local, dev, stg, prod)
        stage: Option<String>,
        /// ステージ名 (-s/--stage フラグ、FLEET_STAGE 環境変数)
        #[arg(
            short = 's',
            long = "stage",
            env = "FLEET_STAGE",
            conflicts_with = "stage",
            hide = true
        )]
        stage_flag: Option<String>,
        /// サービス名（省略時はステージ全体を再起動）
        #[arg(short = 'n', long)]
        service: Option<String>,
    },
    /// コンテナの一覧・状態を表示
    Ps {
        /// ステージ名 (local, dev, stg, prod)
        stage: Option<String>,
        /// ステージ名 (-s/--stage フラグ、FLEET_STAGE 環境変数)
        #[arg(
            short = 's',
            long = "stage",
            env = "FLEET_STAGE",
            conflicts_with = "stage",
            hide = true
        )]
        stage_flag: Option<String>,
        /// 停止中のコンテナも表示
        #[arg(short, long)]
        all: bool,
        /// Control Plane 横断: プロジェクト名で絞り込み
        #[arg(long)]
        project: Option<String>,
        /// Control Plane 横断: 全プロジェクト・全ステージを表示
        #[arg(long)]
        global: bool,
    },
    /// コンテナのログを表示
    Logs {
        /// ステージ名 (local, dev, stg, prod)
        stage: Option<String>,
        /// ステージ名 (-s/--stage フラグ、FLEET_STAGE 環境変数)
        #[arg(
            short = 's',
            long = "stage",
            env = "FLEET_STAGE",
            conflicts_with = "stage",
            hide = true
        )]
        stage_flag: Option<String>,
        /// サービス名（複数指定可、省略時は全サービス）
        #[arg(short = 'n', long)]
        service: Vec<String>,
        /// ログの行数を指定
        #[arg(short = 'l', long, default_value = "100")]
        lines: usize,
        /// ログをリアルタイムで追跡
        #[arg(short, long)]
        follow: bool,
        /// 指定時間以降のログを表示（例: 5m, 1h, 30s）
        #[arg(long)]
        since: Option<String>,
    },
    /// サービスコンテナ内でコマンドを実行
    Exec {
        /// ステージ名 (local, dev, stg, prod)
        stage: Option<String>,
        /// ステージ名 (-s/--stage フラグ、FLEET_STAGE 環境変数)
        #[arg(
            short = 's',
            long = "stage",
            env = "FLEET_STAGE",
            conflicts_with = "stage",
            hide = true
        )]
        stage_flag: Option<String>,
        /// サービス名
        #[arg(short = 'n', long)]
        service: String,
        /// インタラクティブモード（stdinを接続）
        #[arg(short = 'i', long)]
        interactive: bool,
        /// 擬似TTYを割り当て
        #[arg(short = 't', long)]
        tty: bool,
        /// 実行するコマンド（-- 以降）。省略時は /bin/sh
        #[arg(last = true)]
        command: Vec<String>,
    },

    // ── Ship ───────────────────────────────────

    /// Dockerイメージをビルド
    Build {
        /// ステージ名 (local, dev, stg, prod)
        stage: Option<String>,
        /// ステージ名 (-s/--stage フラグ、FLEET_STAGE 環境変数)
        #[arg(
            short = 's',
            long = "stage",
            env = "FLEET_STAGE",
            conflicts_with = "stage",
            hide = true
        )]
        stage_flag: Option<String>,
        /// ビルド対象のサービス（複数指定可、省略時は全サービス）
        #[arg(short = 'n', long)]
        service: Vec<String>,
        /// ビルド後にレジストリにプッシュ
        #[arg(long)]
        push: bool,
        /// イメージタグを指定（--pushと併用）
        #[arg(long)]
        tag: Option<String>,
        /// レジストリURL（例: ghcr.io/owner）
        #[arg(long)]
        registry: Option<String>,
        /// ターゲットプラットフォーム（例: linux/amd64）
        #[arg(long)]
        platform: Option<String>,
        /// キャッシュを使用しない
        #[arg(long)]
        no_cache: bool,
    },
    /// ステージをデプロイ（pull→停止→再起動）
    Deploy {
        /// ステージ名 (local, dev, stg, prod)
        stage: Option<String>,
        /// ステージ名 (-s/--stage フラグ、FLEET_STAGE 環境変数)
        #[arg(
            short = 's',
            long = "stage",
            env = "FLEET_STAGE",
            conflicts_with = "stage",
            hide = true
        )]
        stage_flag: Option<String>,
        /// デプロイ対象のサービス（複数指定可、省略時は全サービス）
        #[arg(short = 'n', long)]
        service: Vec<String>,
        /// イメージのpullをスキップ（デフォルトは常にpull）
        #[arg(long)]
        no_pull: bool,
        /// デプロイ後の不要イメージ・ビルドキャッシュ削除をスキップ
        #[arg(long)]
        no_prune: bool,
        /// 確認なしで実行
        #[arg(short, long)]
        yes: bool,
        /// 実行せずに実行計画のみ表示
        #[arg(long)]
        dry_run: bool,
    },

    // ── Admin ──────────────────────────────────

    /// Control Plane 管理
    #[command(subcommand)]
    Cp(CpCommands),

    // ── Util ───────────────────────────────────

    /// MCP (Model Context Protocol) サーバーを起動
    Mcp,
    /// FleetFlow自体を最新版に更新
    #[command(name = "self-update")]
    SelfUpdate,
}

// ─────────────────────────────────────────────
// CP subcommands — fleet cp <subcommand>
// ─────────────────────────────────────────────

#[derive(Subcommand)]
enum CpCommands {
    /// CP にログイン
    Login {
        /// API エンドポイント
        #[arg(long)]
        endpoint: Option<String>,
    },
    /// CP からログアウト
    Logout,
    /// 認証状態を表示
    Auth,
    /// デーモン管理
    #[command(subcommand)]
    Daemon(DaemonCommands),
    /// テナント管理
    #[command(subcommand)]
    Tenant(TenantCommands),
    /// プロジェクト管理
    #[command(subcommand)]
    Project(ProjectCommands),
    /// サーバー管理
    #[command(subcommand)]
    Server(ServerCommands),
    /// コスト管理
    #[command(subcommand)]
    Cost(CostCommands),
    /// DNS/ドメイン管理
    #[command(subcommand)]
    Dns(DnsCommands),
    /// リモートデプロイ
    #[command(subcommand, name = "remote")]
    Remote(RemoteCommands),
    /// Fleet Registry 管理
    #[command(subcommand)]
    Registry(RegistryCommands),
}

/// デーモン管理のサブコマンド
#[derive(Subcommand)]
enum DaemonCommands {
    /// デーモンを起動（フォアグラウンド）
    Start,
    /// デーモンを停止
    Stop,
    /// デーモンの状態を表示
    Status,
}

/// テナント管理のサブコマンド
#[derive(Subcommand)]
enum TenantCommands {
    /// テナント状態を表示
    Status,
    /// テナント一覧
    List,
    /// テナント作成
    Create {
        /// テナントのスラッグ
        #[arg(long)]
        slug: String,
        /// テナント名
        #[arg(long)]
        name: String,
        /// プラン（self-hosted, starter, pro, enterprise）
        #[arg(long, default_value = "self-hosted")]
        plan: String,
    },
}

/// プロジェクト管理のサブコマンド
#[derive(Subcommand)]
enum ProjectCommands {
    /// プロジェクト一覧
    List,
    /// プロジェクト作成
    Create {
        /// プロジェクトのスラッグ（一意識別子）
        #[arg(long)]
        slug: String,
        /// プロジェクト名
        #[arg(long)]
        name: String,
    },
    /// プロジェクト詳細
    Show {
        /// プロジェクトのスラッグ
        slug: String,
    },
}

/// サーバー管理のサブコマンド
#[derive(Subcommand)]
enum ServerCommands {
    /// サーバー一覧
    List,
    /// サーバー登録
    Register {
        /// サーバーのスラッグ
        #[arg(long)]
        slug: String,
        /// プロバイダー（sakura-cloud, manual 等）
        #[arg(long)]
        provider: String,
        /// SSH ホスト（IP or hostname）
        #[arg(long)]
        ssh_host: Option<String>,
        /// デプロイ先パス
        #[arg(long)]
        deploy_path: Option<String>,
    },
    /// サーバー状態
    Status {
        /// サーバーのスラッグ
        slug: String,
    },
    /// 全サーバーのヘルスチェック（Tailscale 経由）
    Check,
    /// 指定サーバーに Tailscale ping
    Ping {
        /// サーバーのホスト名（Tailscale ノード名）
        hostname: String,
    },
}

/// コスト管理のサブコマンド
#[derive(Subcommand)]
enum CostCommands {
    /// 月次コスト一覧
    List {
        /// 対象年月（例: 2026-03）
        #[arg(long)]
        month: String,
    },
    /// コスト集計（プロバイダ×プロジェクト別）
    Summary {
        /// 対象年月（例: 2026-03）
        #[arg(long)]
        month: String,
    },
    /// コストエントリ登録
    Record {
        /// プロバイダ種別（sakura, cloudflare, auth0, stripe, other）
        #[arg(long)]
        provider: String,
        /// 金額（円）
        #[arg(long)]
        amount: i64,
        /// 対象年月（例: 2026-03）
        #[arg(long)]
        month: String,
        /// コスト説明
        #[arg(long)]
        description: String,
        /// 帰属プロジェクト（オプション）
        #[arg(long)]
        project: Option<String>,
        /// 帰属ステージ（オプション）
        #[arg(long)]
        stage: Option<String>,
    },
}

/// DNS/ドメイン管理のサブコマンド
#[derive(Subcommand)]
enum DnsCommands {
    /// DNS レコード一覧
    List,
    /// DNS レコード作成
    Create {
        /// ドメイン名（例: api.example.com）
        #[arg(long)]
        name: String,
        /// レコードタイプ（A, AAAA, CNAME, TXT）
        #[arg(long, default_value = "A")]
        record_type: String,
        /// レコード値
        #[arg(long)]
        content: String,
        /// Cloudflare プロキシ有効
        #[arg(long)]
        proxied: bool,
        /// 帰属プロジェクト（オプション）
        #[arg(long)]
        project: Option<String>,
    },
    /// DNS レコード削除
    Delete {
        /// ドメイン名
        name: String,
    },
    /// Cloudflare DNS と同期
    Sync,
}

/// リモートデプロイのサブコマンド
#[derive(Subcommand)]
enum RemoteCommands {
    /// リモートサーバーにデプロイ実行
    Deploy {
        /// プロジェクトスラッグ
        #[arg(long)]
        project: String,
        /// ステージ名
        #[arg(long)]
        stage: String,
        /// 対象サーバースラッグ
        #[arg(long)]
        server: String,
        /// 実行コマンド
        #[arg(long)]
        command: String,
    },
    /// デプロイ履歴
    History {
        /// 表示件数
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}

/// Fleet Registryのサブコマンド
#[derive(Subcommand)]
enum RegistryCommands {
    /// 全fleetとサーバーの一覧を表示
    List,
    /// 各fleet × serverの稼働状態を表示（CP接続時は実稼働状態）
    Status,
    /// Registry定義をControl Planeに同期
    Sync,
    /// Registry定義に従ってデプロイ
    Deploy {
        /// デプロイ対象のfleet名
        fleet: String,
        /// ステージ名
        #[arg(short = 's', long)]
        stage: Option<String>,
        /// 確認なしで実行
        #[arg(short, long)]
        yes: bool,
    },
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

/// stage 位置引数と -s フラグを統合する
fn resolve_stage(positional: Option<String>, flag: Option<String>) -> Option<String> {
    positional.or(flag)
}

// ─────────────────────────────────────────────
// main
// ─────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // ── MCP: stdout を JSON-RPC に使うので先に処理 ──
    if matches!(cli.command, Commands::Mcp) {
        use std::fs::OpenOptions;
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/fleetflow-mcp.log")
            .ok();

        if let Some(file) = log_file {
            tracing_subscriber::fmt()
                .with_writer(file)
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive(tracing::Level::DEBUG.into()),
                )
                .with_ansi(false)
                .init();
        }

        return fleetflow_mcp::run_server().await;
    }

    // ── ロギング初期化 ──
    let log_level = if cli.verbose {
        "debug"
    } else if cli.quiet {
        "error"
    } else {
        ""
    };

    if log_level.is_empty() {
        tracing_subscriber::fmt::init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(log_level))
            .init();
    }

    // ── 設定ファイル不要なコマンド ──
    if matches!(cli.command, Commands::SelfUpdate) {
        return self_update::self_update().await;
    }

    // CP コマンドは設定ファイル不要
    if let Commands::Cp(ref cp_cmd) = cli.command {
        return handle_cp(cp_cmd).await;
    }

    // CP 横断クエリ（--project / --global）
    match &cli.command {
        Commands::Ps {
            project: Some(project),
            stage,
            stage_flag,
            ..
        } => {
            let stage_name = stage.as_deref().or(stage_flag.as_deref());
            return commands::ps::handle_cp_query(Some(project), stage_name).await;
        }
        Commands::Ps { global: true, .. } => {
            return commands::ps::handle_cp_query(None, None).await;
        }
        _ => {}
    }

    // ── プロジェクトルート検索 ──
    let project_root = match fleetflow_core::find_project_root() {
        Ok(root) => root,
        Err(fleetflow_core::FlowError::ProjectRootNotFound(_)) => {
            println!("{}", "設定ファイルが見つかりません。".yellow());
            println!("{}", "初期化ウィザードを起動します...".cyan());
            println!();

            match tui::run_init_wizard()? {
                Some((path, content)) => {
                    let config_path = if path.starts_with("~/") {
                        let home = dirs::home_dir()
                            .ok_or_else(|| anyhow::anyhow!("ホームディレクトリが見つかりません"))?;
                        PathBuf::from(path.replace("~/", &format!("{}/", home.display())))
                    } else {
                        PathBuf::from(&path)
                    };

                    if let Some(parent) = config_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }

                    std::fs::write(&config_path, content)?;

                    println!();
                    println!("{}", "✓ 設定ファイルを作成しました！".green());
                    println!("  {}", config_path.display().to_string().cyan());
                    println!();
                    println!("{}", "次のコマンドで環境を起動できます:".bold());
                    println!("  {} up", "fleet".cyan());

                    return Ok(());
                }
                None => {
                    println!("{}", "初期化をキャンセルしました。".yellow());
                    return Ok(());
                }
            }
        }
        Err(e) => return Err(e.into()),
    };

    // ── stage ヒント抽出 & 設定ロード ──
    let stage_from_env = std::env::var("FLEET_STAGE").ok();
    let stage_name_hint: Option<&str> = match &cli.command {
        Commands::Up {
            stage, stage_flag, ..
        }
        | Commands::Down {
            stage, stage_flag, ..
        }
        | Commands::Restart {
            stage, stage_flag, ..
        }
        | Commands::Ps {
            stage, stage_flag, ..
        }
        | Commands::Logs {
            stage, stage_flag, ..
        }
        | Commands::Exec {
            stage, stage_flag, ..
        }
        | Commands::Build {
            stage, stage_flag, ..
        }
        | Commands::Deploy {
            stage, stage_flag, ..
        } => stage.as_deref().or(stage_flag.as_deref()),
        _ => stage_from_env.as_deref(),
    };

    if let Some(stage) = stage_name_hint {
        unsafe {
            std::env::set_var("FLEET_STAGE", stage);
        }
    }

    let config = match fleetflow_core::load_project_from_root_with_stage(
        &project_root,
        stage_name_hint,
    ) {
        Ok(config) => config,
        Err(ref e)
            if stage_name_hint.is_none()
                && matches!(
                    e,
                    fleetflow_core::FlowError::TemplateError { .. }
                        | fleetflow_core::FlowError::TemplateRenderError(_)
                ) =>
        {
            eprintln!(
                "{} ステージが指定されていません。変数を含む設定ファイルの読み込みにはステージの指定が必要です。",
                "Error:".red().bold()
            );
            eprintln!();
            eprintln!(
                "{}",
                "ヒント: 以下のいずれかの方法でステージを指定してください:".yellow()
            );
            eprintln!("  fleet <command> <stage>              例: fleet ps prod");
            eprintln!("  fleet <command> -s <stage>           例: fleet ps -s prod");
            eprintln!("  FLEET_STAGE=<stage> fleet <command>  例: FLEET_STAGE=prod fleet ps");
            std::process::exit(1);
        }
        Err(e) => return Err(e.into()),
    };

    // ── コマンドディスパッチ ──
    match cli.command {
        // Daily
        Commands::Up {
            stage,
            stage_flag,
            pull,
            dry_run,
        } => {
            let stage = resolve_stage(stage, stage_flag);
            commands::up::handle(&config, &project_root, stage, pull, dry_run).await?;
        }
        Commands::Down {
            stage,
            stage_flag,
            remove,
        } => {
            let stage = resolve_stage(stage, stage_flag);
            commands::down::handle(&config, &project_root, stage, remove).await?;
        }
        Commands::Restart {
            stage,
            stage_flag,
            service,
        } => {
            let stage = resolve_stage(stage, stage_flag);
            commands::restart::handle(&config, service, stage).await?;
        }
        Commands::Ps {
            stage,
            stage_flag,
            all,
            ..
        } => {
            let stage = resolve_stage(stage, stage_flag);
            commands::ps::handle(&config, &project_root, stage, all).await?;
        }
        Commands::Logs {
            stage,
            stage_flag,
            service,
            lines,
            follow,
            since,
        } => {
            let stage = resolve_stage(stage, stage_flag);
            commands::logs::handle(&config, &project_root, stage, &service, lines, follow, since)
                .await?;
        }
        Commands::Exec {
            stage,
            stage_flag,
            service,
            interactive,
            tty,
            command,
        } => {
            let stage = resolve_stage(stage, stage_flag);
            commands::exec::handle(&config, stage, service, command, interactive, tty).await?;
        }

        // Ship
        Commands::Build {
            stage,
            stage_flag,
            service,
            push,
            tag,
            registry,
            platform,
            no_cache,
        } => {
            let stage = resolve_stage(stage, stage_flag);
            let stage_name = utils::determine_stage_name(stage, &config)?;
            build::handle_build_command(
                &project_root,
                &config,
                &stage_name,
                &service,
                push,
                tag.as_deref(),
                registry.as_deref(),
                platform.as_deref(),
                no_cache,
            )
            .await?;
        }
        Commands::Deploy {
            stage,
            stage_flag,
            service,
            no_pull,
            no_prune,
            yes,
            dry_run,
        } => {
            let stage = resolve_stage(stage, stage_flag);
            commands::deploy::handle(
                &config,
                &project_root,
                stage,
                &service,
                no_pull,
                no_prune,
                yes,
                dry_run,
            )
            .await?;
        }

        // Util
        Commands::Mcp => unreachable!("handled before config loading"),
        Commands::SelfUpdate => unreachable!("handled before config loading"),
        Commands::Cp(_) => unreachable!("handled before config loading"),
    }

    Ok(())
}

// ─────────────────────────────────────────────
// CP command handler
// ─────────────────────────────────────────────

async fn handle_cp(cmd: &CpCommands) -> anyhow::Result<()> {
    match cmd {
        CpCommands::Login { endpoint } => {
            commands::auth::handle_login(endpoint.clone()).await
        }
        CpCommands::Logout => commands::auth::handle_logout().await,
        CpCommands::Auth => commands::auth::handle_auth_status().await,
        CpCommands::Daemon(daemon_cmd) => commands::daemon::handle(daemon_cmd).await,
        CpCommands::Tenant(tenant_cmd) => commands::cp::handle_tenant(tenant_cmd).await,
        CpCommands::Project(project_cmd) => commands::cp::handle_project(project_cmd).await,
        CpCommands::Server(server_cmd) => commands::cp::handle_server(server_cmd).await,
        CpCommands::Cost(cost_cmd) => commands::cp::handle_cost(cost_cmd).await,
        CpCommands::Dns(dns_cmd) => commands::cp::handle_dns(dns_cmd).await,
        CpCommands::Remote(remote_cmd) => commands::cp::handle_remote(remote_cmd).await,
        CpCommands::Registry(registry_cmd) => {
            let (registry, root) = commands::registry::load_registry()?;
            match registry_cmd {
                RegistryCommands::List => {
                    commands::registry::handle_list(&registry);
                }
                RegistryCommands::Status => {
                    commands::registry::handle_status(&registry);
                }
                RegistryCommands::Sync => {
                    commands::registry::handle_sync(&registry).await?;
                }
                RegistryCommands::Deploy { fleet, stage, yes } => {
                    commands::registry::handle_deploy(
                        &registry,
                        &root,
                        fleet,
                        stage.as_deref(),
                        *yes,
                    )
                    .await?;
                }
            }
            Ok(())
        }
    }
}
