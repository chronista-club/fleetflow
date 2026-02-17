mod build;
mod commands;
mod docker;
mod play;
mod self_update;
mod tui;
mod utils;

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fleet")]
#[command(about = "伝える。動く。環境構築は、対話になった。", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
        /// サービス名（指定しない場合は全サービス）
        #[arg(short = 'n', long)]
        service: Option<String>,
        /// ログの行数を指定
        #[arg(short = 'l', long, default_value = "100")]
        lines: usize,
        /// ログをリアルタイムで追跡
        #[arg(short, long)]
        follow: bool,
    },
    /// コンテナの一覧を表示
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
        /// 実行するコマンド（-- 以降）。省略時は /bin/sh
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// サービスを再起動
    Restart {
        /// サービス名
        service: String,
        /// ステージ名 (local, dev, stg, prod)
        /// 環境変数 FLEET_STAGE または --stage オプションで指定
        #[arg(short, long, env = "FLEET_STAGE")]
        stage: Option<String>,
    },
    /// サービスを停止
    Stop {
        /// サービス名
        service: String,
        /// ステージ名 (local, dev, stg, prod)
        /// 環境変数 FLEET_STAGE または --stage オプションで指定
        #[arg(short, long, env = "FLEET_STAGE")]
        stage: Option<String>,
    },
    /// サービスを起動
    Start {
        /// サービス名
        service: String,
        /// ステージ名 (local, dev, stg, prod)
        /// 環境変数 FLEET_STAGE または --stage オプションで指定
        #[arg(short, long, env = "FLEET_STAGE")]
        stage: Option<String>,
    },
    /// 設定を検証
    Validate {
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
    },
    /// バージョン情報を表示
    Version,
    /// FleetFlow自体を最新版に更新
    #[command(name = "self-update")]
    SelfUpdate,
    /// ステージをデプロイ（CI/CD向け）
    /// 既存コンテナを強制停止・削除し、最新イメージで再起動
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
        /// デプロイ対象のサービス（省略時は全サービス）
        #[arg(short = 'n', long)]
        service: Option<String>,
        /// イメージのpullをスキップ（デフォルトは常にpull）
        #[arg(long)]
        no_pull: bool,
        /// デプロイ後の不要イメージ・ビルドキャッシュ削除をスキップ
        #[arg(long)]
        no_prune: bool,
        /// 確認なしで実行
        #[arg(short, long)]
        yes: bool,
    },
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
        /// ビルド対象のサービス（省略時は全サービス）
        #[arg(short = 'n', long)]
        service: Option<String>,
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
    /// ステージを管理（インフラ＋コンテナを統一的に操作）
    #[command(subcommand)]
    Stage(StageCommands),
    /// MCP (Model Context Protocol) サーバーを起動
    Mcp,
    /// Playbookを実行（リモートサーバーでサービスを起動）
    Play {
        /// Playbook名
        playbook: String,
        /// 確認なしで実行
        #[arg(short, long)]
        yes: bool,
        /// 起動前に最新イメージをpullする
        #[arg(long)]
        pull: bool,
    },
    /// Fleet Registryを管理（複数fleetとサーバーの統合管理）
    #[command(subcommand)]
    Registry(RegistryCommands),
}

/// Fleet Registryのサブコマンド
#[derive(Subcommand)]
enum RegistryCommands {
    /// 全fleetとサーバーの一覧を表示
    List,
    /// 各fleet × serverの稼働状態を表示
    Status,
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

/// ステージ管理のサブコマンド
#[derive(Subcommand)]
enum StageCommands {
    /// ステージを起動（インフラ＋コンテナ）
    Up {
        /// ステージ名 (local, dev, pre, prod)
        stage: String,
        /// 確認プロンプトをスキップ
        #[arg(short = 'y', long)]
        yes: bool,
        /// 起動前に最新イメージをpullする
        #[arg(long)]
        pull: bool,
    },
    /// ステージを停止
    Down {
        /// ステージ名 (local, dev, pre, prod)
        stage: String,
        /// サーバー電源をOFFにする（リモートステージのみ）
        #[arg(long)]
        suspend: bool,
        /// サーバーを削除する（⚠️ 課金完全停止、データ削除）
        #[arg(long)]
        destroy: bool,
        /// 確認プロンプトをスキップ
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// ステージの状態を表示
    Status {
        /// ステージ名 (local, dev, pre, prod)
        stage: String,
    },
    /// ログを表示
    Logs {
        /// ステージ名 (local, dev, pre, prod)
        stage: String,
        /// 特定サービスのログのみ
        #[arg(short, long)]
        service: Option<String>,
        /// リアルタイム追従
        #[arg(short, long)]
        follow: bool,
        /// 最新N行
        #[arg(short = 'n', long, default_value = "100")]
        tail: usize,
    },
    /// コンテナ一覧
    Ps {
        /// ステージ名（省略時は全ステージ）
        stage: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Mcpコマンドは設定ファイル不要（ツール実行時に必要に応じてロード）
    // stdoutはJSON-RPC通信に使うので、ログはファイルに出力
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

        // rmcp SDK ベースの MCP サーバーを起動（stdio トランスポート）
        return fleetflow_mcp::run_server().await;
    }

    // 通常のCLIコマンドはstderrにログ出力
    tracing_subscriber::fmt::init();

    // Versionコマンドは設定ファイル不要
    if matches!(cli.command, Commands::Version) {
        println!("fleetflow {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // SelfUpdateコマンドは設定ファイル不要
    if matches!(cli.command, Commands::SelfUpdate) {
        return self_update::self_update().await;
    }

    // Registryコマンドは独自のファイル発見ロジックを使用
    if let Commands::Registry(ref registry_cmd) = cli.command {
        let (registry, root) = commands::registry::load_registry()?;
        match registry_cmd {
            RegistryCommands::List => {
                commands::registry::handle_list(&registry);
            }
            RegistryCommands::Status => {
                commands::registry::handle_status(&registry);
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
        return Ok(());
    }

    // プロジェクトルートを検索
    let project_root = match fleetflow_core::find_project_root() {
        Ok(root) => root,
        Err(fleetflow_core::FlowError::ProjectRootNotFound(_)) => {
            // 設定ファイルが見つからない場合は初期化ウィザードを起動
            println!("{}", "設定ファイルが見つかりません。".yellow());
            println!("{}", "初期化ウィザードを起動します...".cyan());
            println!();

            match tui::run_init_wizard()? {
                Some((path, content)) => {
                    // 設定ファイルを作成
                    let config_path = if path.starts_with("~/") {
                        let home = dirs::home_dir()
                            .ok_or_else(|| anyhow::anyhow!("ホームディレクトリが見つかりません"))?;
                        PathBuf::from(path.replace("~/", &format!("{}/", home.display())))
                    } else {
                        PathBuf::from(&path)
                    };

                    // ディレクトリが存在しない場合は作成
                    if let Some(parent) = config_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }

                    // ファイルを書き込み
                    std::fs::write(&config_path, content)?;

                    println!();
                    println!("{}", "✓ 設定ファイルを作成しました！".green());
                    println!("  {}", config_path.display().to_string().cyan());
                    println!();
                    println!("{}", "次のコマンドで環境を起動できます:".bold());
                    println!("  {} up", "fleetflow".cyan());

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

    // プロジェクト全体をロード（fleet.kdl + stage固有設定 + localを自動マージ）
    let stage_from_env = std::env::var("FLEET_STAGE").ok();
    let stage_name_hint: Option<&str> = match &cli.command {
        Commands::Up {
            stage, stage_flag, ..
        } => stage.as_deref().or(stage_flag.as_deref()),
        Commands::Down {
            stage, stage_flag, ..
        } => stage.as_deref().or(stage_flag.as_deref()),
        Commands::Logs {
            stage, stage_flag, ..
        } => stage.as_deref().or(stage_flag.as_deref()),
        Commands::Ps {
            stage, stage_flag, ..
        } => stage.as_deref().or(stage_flag.as_deref()),
        Commands::Exec {
            stage, stage_flag, ..
        } => stage.as_deref().or(stage_flag.as_deref()),
        Commands::Restart { stage, .. } => stage.as_deref(),
        Commands::Stop { stage, .. } => stage.as_deref(),
        Commands::Start { stage, .. } => stage.as_deref(),
        Commands::Validate {
            stage, stage_flag, ..
        } => stage.as_deref().or(stage_flag.as_deref()),
        Commands::Deploy {
            stage, stage_flag, ..
        } => stage.as_deref().or(stage_flag.as_deref()),
        Commands::Build {
            stage, stage_flag, ..
        } => stage.as_deref().or(stage_flag.as_deref()),
        Commands::Stage(stage_cmd) => match stage_cmd {
            StageCommands::Up { stage, .. } => Some(stage.as_str()),
            StageCommands::Down { stage, .. } => Some(stage.as_str()),
            StageCommands::Status { stage } => Some(stage.as_str()),
            StageCommands::Logs { stage, .. } => Some(stage.as_str()),
            StageCommands::Ps { stage } => stage.as_deref(),
        },
        Commands::Registry(_) => None,
        _ => stage_from_env.as_deref(),
    };

    // --stage オプションで指定された場合、環境変数 FLEET_STAGE を設定
    if let Some(stage) = stage_name_hint {
        // SAFETY: 環境変数は単一スレッドで設定されるため安全
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

    // コマンドディスパッチ
    match cli.command {
        Commands::Up {
            stage,
            stage_flag,
            pull,
        } => {
            let stage = stage.or(stage_flag);
            commands::up::handle(&config, &project_root, stage, pull).await?;
        }
        Commands::Down {
            stage,
            stage_flag,
            remove,
        } => {
            let stage = stage.or(stage_flag);
            commands::down::handle(&config, &project_root, stage, remove).await?;
        }
        Commands::Ps {
            stage,
            stage_flag,
            all,
        } => {
            let stage = stage.or(stage_flag);
            commands::ps::handle(&config, &project_root, stage, all).await?;
        }
        Commands::Logs {
            stage,
            stage_flag,
            service,
            lines,
            follow,
        } => {
            let stage = stage.or(stage_flag);
            commands::logs::handle(&config, &project_root, stage, service, lines, follow).await?;
        }
        Commands::Restart { service, stage } => {
            commands::restart::handle(&config, service, stage).await?;
        }
        Commands::Stop { service, stage } => {
            commands::stop::handle(&config, service, stage).await?;
        }
        Commands::Start { service, stage } => {
            commands::start::handle(&config, service, stage).await?;
        }
        Commands::Deploy {
            stage,
            stage_flag,
            service,
            no_pull,
            no_prune,
            yes,
        } => {
            let stage = stage.or(stage_flag);
            commands::deploy::handle(
                &config,
                &project_root,
                stage,
                service,
                no_pull,
                no_prune,
                yes,
            )
            .await?;
        }
        Commands::Validate {
            stage: _,
            stage_flag: _,
        } => {
            commands::validate::handle().await?;
        }
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
            let stage = stage.or(stage_flag);
            let stage_name = utils::determine_stage_name(stage, &config)?;
            build::handle_build_command(
                &project_root,
                &config,
                &stage_name,
                service.as_deref(),
                push,
                tag.as_deref(),
                registry.as_deref(),
                platform.as_deref(),
                no_cache,
            )
            .await?;
        }
        Commands::Stage(stage_cmd) => {
            commands::stage::handle(stage_cmd, &project_root, &config).await?;
        }
        Commands::Mcp => {
            unreachable!("Mcp is handled before config loading");
        }
        Commands::SelfUpdate => {
            unreachable!("SelfUpdate is handled before config loading");
        }
        Commands::Play {
            playbook,
            yes,
            pull,
        } => {
            play::handle_play_command(&project_root, &playbook, yes, pull).await?;
        }
        Commands::Version => {
            unreachable!("Version is handled before config loading");
        }
        Commands::Exec {
            stage,
            stage_flag,
            service,
            command,
        } => {
            let stage = stage.or(stage_flag);
            commands::exec::handle(&config, stage, service, command).await?;
        }
        Commands::Registry(_) => {
            unreachable!("Registry is handled before config loading");
        }
    }

    Ok(())
}
