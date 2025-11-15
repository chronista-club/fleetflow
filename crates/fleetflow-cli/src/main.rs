mod tui;

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

/// Docker接続を初期化（エラーハンドリング付き）
async fn init_docker_with_error_handling() -> anyhow::Result<bollard::Docker> {
    match bollard::Docker::connect_with_local_defaults() {
        Ok(docker) => {
            // 接続テスト
            match docker.ping().await {
                Ok(_) => Ok(docker),
                Err(e) => {
                    eprintln!();
                    eprintln!("{}", "✗ Docker接続エラー".red().bold());
                    eprintln!();
                    eprintln!("{}", "原因:".yellow());
                    eprintln!("  {}", e);
                    eprintln!();
                    eprintln!("{}", "解決方法:".yellow());
                    eprintln!("  • Dockerが起動しているか確認してください");
                    eprintln!(
                        "  • OrbStackまたはDocker Desktopがインストールされているか確認してください"
                    );
                    eprintln!("  • docker ps コマンドが正常に動作するか確認してください");
                    Err(anyhow::anyhow!("Docker接続に失敗しました"))
                }
            }
        }
        Err(e) => {
            eprintln!();
            eprintln!("{}", "✗ Docker接続エラー".red().bold());
            eprintln!();
            eprintln!("{}", "原因:".yellow());
            eprintln!("  {}", e);
            eprintln!();
            eprintln!("{}", "解決方法:".yellow());
            eprintln!("  • Dockerが起動しているか確認してください");
            eprintln!("  • OrbStackまたはDocker Desktopがインストールされているか確認してください");
            eprintln!("  • docker ps コマンドが正常に動作するか確認してください");
            Err(anyhow::anyhow!("Docker接続に失敗しました"))
        }
    }
}

#[derive(Parser)]
#[command(name = "flow")]
#[command(about = "Docker Composeよりシンプル。KDLで書く、次世代の環境構築ツール。", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// ステージを起動
    Up {
        /// ステージ名を指定 (local, dev, stg, prd)
        /// 環境変数 FLOW_STAGE からも読み込み可能
        #[arg(short, long, env = "FLOW_STAGE")]
        stage: Option<String>,
    },
    /// ステージを停止
    Down {
        /// ステージ名を指定 (local, dev, stg, prd)
        /// 環境変数 FLOW_STAGE からも読み込み可能
        #[arg(short, long, env = "FLOW_STAGE")]
        stage: Option<String>,
        /// コンテナを削除する（デフォルトは停止のみ）
        #[arg(short, long)]
        remove: bool,
    },
    /// コンテナのログを表示
    Logs {
        /// ステージ名を指定 (local, dev, stg, prd)
        /// 環境変数 FLOW_STAGE からも読み込み可能
        #[arg(short, long, env = "FLOW_STAGE")]
        stage: Option<String>,
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
        /// ステージ名を指定 (local, dev, stg, prd)
        /// 環境変数 FLOW_STAGE からも読み込み可能
        #[arg(short, long, env = "FLOW_STAGE")]
        stage: Option<String>,
        /// 停止中のコンテナも表示
        #[arg(short, long)]
        all: bool,
    },
    /// 設定を検証
    Validate,
    /// バージョン情報を表示
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Versionコマンドは設定ファイル不要
    if matches!(cli.command, Commands::Version) {
        println!("fleetflow {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // 設定ファイルを検索
    let config_path = match fleetflow_config::find_flow_file() {
        Ok(path) => path,
        Err(fleetflow_config::ConfigError::FlowFileNotFound) => {
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
                    println!("  {} up", "flow".cyan());

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

    // 設定ファイルをパース
    let config = fleetflow_atom::parse_kdl_file(&config_path)?;

    // ここから既存のコマンド処理
    match cli.command {
        Commands::Up { stage } => {
            println!("{}", "ステージを起動中...".green());
            println!("設定ファイル: {}", config_path.display().to_string().cyan());

            // ステージ名の決定（デフォルトステージをサポート）
            let stage_name = if let Some(s) = stage {
                s
            } else if config.stages.contains_key("default") {
                "default".to_string()
            } else if config.stages.len() == 1 {
                config.stages.keys().next().unwrap().clone()
            } else {
                return Err(anyhow::anyhow!(
                    "ステージ名を指定してください: --stage=<stage> または FLOW_STAGE=<stage>\n利用可能なステージ: {}",
                    config
                        .stages
                        .keys()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            };

            println!("ステージ: {}", stage_name.cyan());

            // ステージの取得
            let stage_config = config
                .stages
                .get(&stage_name)
                .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

            println!();
            println!(
                "{}",
                format!("サービス一覧 ({} 個):", stage_config.services.len()).bold()
            );
            for service_name in &stage_config.services {
                println!("  • {}", service_name.cyan());
            }

            // Docker接続
            println!();
            println!("{}", "Dockerに接続中...".blue());
            let docker = init_docker_with_error_handling().await?;

            // 各サービスを起動
            for service_name in &stage_config.services {
                let service = config.services.get(service_name).ok_or_else(|| {
                    anyhow::anyhow!("サービス '{}' の定義が見つかりません", service_name)
                })?;

                println!();
                println!(
                    "{}",
                    format!("▶ {} を起動中...", service_name).green().bold()
                );

                // サービスをコンテナ設定に変換
                let (container_config, create_options) =
                    fleetflow_container::service_to_container_config(service_name, service);

                // コンテナ作成
                match docker
                    .create_container(Some(create_options.clone()), container_config.clone())
                    .await
                {
                    Ok(response) => {
                        println!("  ✓ コンテナ作成: {}", response.id);

                        // コンテナ起動
                        match docker.start_container::<String>(&response.id, None).await {
                            Ok(_) => println!("  ✓ 起動完了"),
                            Err(e) => {
                                eprintln!("  ✗ 起動エラー: {}", e);
                                return Err(anyhow::anyhow!("コンテナ起動に失敗しました"));
                            }
                        }
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 409,
                        ..
                    }) => {
                        // コンテナが既に存在する場合
                        println!("  ℹ コンテナは既に存在します");
                        let container_name = &create_options.name;

                        // 既存コンテナを起動
                        match docker.start_container::<String>(container_name, None).await {
                            Ok(_) => println!("  ✓ 既存コンテナを起動"),
                            Err(bollard::errors::Error::DockerResponseServerError {
                                status_code: 304,
                                ..
                            }) => {
                                println!("  ℹ コンテナは既に起動しています");
                            }
                            Err(e) => {
                                eprintln!("  ✗ 起動エラー: {}", e);
                                return Err(anyhow::anyhow!("コンテナ起動に失敗しました"));
                            }
                        }
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        message,
                    }) => {
                        eprintln!();
                        eprintln!("{}", "✗ イメージが見つかりません".red().bold());
                        eprintln!();
                        eprintln!("{}", "原因:".yellow());
                        eprintln!("  {}", message);
                        eprintln!();
                        eprintln!("{}", "解決方法:".yellow());
                        let image = service
                            .image
                            .as_ref()
                            .cloned()
                            .unwrap_or_else(|| format!("{}:latest", service_name));
                        eprintln!("  • イメージをダウンロード: docker pull {}", image);
                        eprintln!("  • イメージ名とタグを確認してください");
                        return Err(anyhow::anyhow!("イメージが見つかりません"));
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("port is already allocated") {
                            eprintln!();
                            eprintln!("{}", "✗ ポートが既に使用されています".red().bold());
                            eprintln!();
                            eprintln!("{}", "原因:".yellow());
                            eprintln!("  {}", err_str);
                            eprintln!();
                            eprintln!("{}", "解決方法:".yellow());
                            eprintln!("  • 既存のコンテナを停止: flow down --stage={}", stage_name);
                            eprintln!("  • 別のポート番号を使用してください");
                            eprintln!(
                                "  • docker ps でポートを使用しているコンテナを確認してください"
                            );
                        } else {
                            eprintln!();
                            eprintln!("{}", "✗ コンテナ作成エラー".red().bold());
                            eprintln!();
                            eprintln!("{}", "原因:".yellow());
                            eprintln!("  {}", err_str);
                        }
                        return Err(anyhow::anyhow!("コンテナ作成に失敗しました"));
                    }
                }
            }

            println!();
            println!("{}", "✓ すべてのサービスが起動しました！".green().bold());
        }
        Commands::Down { stage, remove } => {
            println!("{}", "ステージを停止中...".yellow());
            println!("設定ファイル: {}", config_path.display().to_string().cyan());

            // ステージ名の決定（デフォルトステージをサポート）
            let stage_name = if let Some(s) = stage {
                s
            } else if config.stages.contains_key("default") {
                "default".to_string()
            } else if config.stages.len() == 1 {
                config.stages.keys().next().unwrap().clone()
            } else {
                return Err(anyhow::anyhow!(
                    "ステージ名を指定してください: --stage=<stage> または FLOW_STAGE=<stage>\n利用可能なステージ: {}",
                    config
                        .stages
                        .keys()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            };

            println!("ステージ: {}", stage_name.cyan());

            // ステージの取得
            let stage_config = config
                .stages
                .get(&stage_name)
                .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

            println!();
            println!(
                "{}",
                format!("サービス一覧 ({} 個):", stage_config.services.len()).bold()
            );
            for service_name in &stage_config.services {
                println!("  • {}", service_name.cyan());
            }

            // Docker接続
            println!();
            println!("{}", "Dockerに接続中...".blue());
            let docker = init_docker_with_error_handling().await?;

            // 各サービスを停止
            for service_name in &stage_config.services {
                println!();
                println!(
                    "{}",
                    format!("■ {} を停止中...", service_name).yellow().bold()
                );

                let container_name = format!("flow-{}", service_name);

                // コンテナを停止
                match docker.stop_container(&container_name, None).await {
                    Ok(_) => {
                        println!("  ✓ 停止完了");

                        // --remove フラグが指定されている場合は削除
                        if remove {
                            match docker.remove_container(&container_name, None).await {
                                Ok(_) => println!("  ✓ 削除完了"),
                                Err(e) => println!("  ⚠ 削除エラー: {}", e),
                            }
                        }
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 304,
                        ..
                    }) => {
                        println!("  ℹ コンテナは既に停止しています");

                        // --remove フラグが指定されている場合は削除
                        if remove {
                            match docker.remove_container(&container_name, None).await {
                                Ok(_) => println!("  ✓ 削除完了"),
                                Err(e) => println!("  ⚠ 削除エラー: {}", e),
                            }
                        }
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        ..
                    }) => {
                        println!("  ℹ コンテナが見つかりません");
                    }
                    Err(e) => {
                        println!("  ⚠ 停止エラー: {}", e);
                    }
                }
            }

            println!();
            if remove {
                println!(
                    "{}",
                    "✓ すべてのサービスが停止・削除されました！".green().bold()
                );
            } else {
                println!("{}", "✓ すべてのサービスが停止しました！".green().bold());
                println!(
                    "{}",
                    "  コンテナを削除するには --remove フラグを使用してください".dimmed()
                );
            }
        }
        Commands::Ps { stage, all } => {
            println!("{}", "コンテナ一覧を取得中...".blue());
            println!("設定ファイル: {}", config_path.display().to_string().cyan());

            // Docker接続
            let docker = init_docker_with_error_handling().await?;

            // コンテナ一覧を取得
            let filters = if let Some(stage_name) = stage {
                println!("ステージ: {}", stage_name.cyan());

                // ステージに属するサービスのみフィルタ
                let stage_config = config
                    .stages
                    .get(&stage_name)
                    .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

                let mut filter_map = std::collections::HashMap::new();
                let names: Vec<String> = stage_config
                    .services
                    .iter()
                    .map(|s| format!("flow-{}", s))
                    .collect();
                filter_map.insert("name".to_string(), names);
                Some(filter_map)
            } else {
                // flow- プレフィックスのコンテナのみ
                let mut filter_map = std::collections::HashMap::new();
                filter_map.insert("name".to_string(), vec!["flow-".to_string()]);
                Some(filter_map)
            };

            let options = bollard::container::ListContainersOptions {
                all,
                filters: filters.unwrap_or_default(),
                ..Default::default()
            };

            let containers = docker.list_containers(Some(options)).await?;

            println!();
            if containers.is_empty() {
                println!("{}", "実行中のコンテナはありません".dimmed());
            } else {
                println!(
                    "{}",
                    format!(
                        "{:<20} {:<15} {:<20} {:<50}",
                        "NAME", "STATUS", "IMAGE", "PORTS"
                    )
                    .bold()
                );
                println!("{}", "─".repeat(105).dimmed());

                for container in containers {
                    let name = container
                        .names
                        .as_ref()
                        .and_then(|n| n.first())
                        .map(|n| n.trim_start_matches('/'))
                        .unwrap_or("N/A");

                    let status = container.status.as_deref().unwrap_or("N/A");
                    let status_colored = if status.contains("Up") {
                        status.green()
                    } else {
                        status.red()
                    };

                    let image = container.image.as_deref().unwrap_or("N/A");

                    let ports = container
                        .ports
                        .as_ref()
                        .map(|ports| {
                            ports
                                .iter()
                                .filter_map(|p| {
                                    p.public_port
                                        .map(|pub_port| format!("{}:{}", pub_port, p.private_port))
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();

                    println!(
                        "{:<20} {:<15} {:<20} {:<50}",
                        name.cyan(),
                        status_colored,
                        image,
                        ports.dimmed()
                    );
                }
            }
        }
        Commands::Logs {
            stage,
            service,
            lines,
            follow,
        } => {
            println!("{}", "ログを取得中...".blue());
            println!("設定ファイル: {}", config_path.display().to_string().cyan());

            // Docker接続
            let docker = init_docker_with_error_handling().await?;

            // 対象サービスの決定
            let target_services = if let Some(service_name) = service {
                vec![service_name]
            } else if let Some(stage_name) = stage {
                println!("ステージ: {}", stage_name.cyan());

                let stage_config = config
                    .stages
                    .get(&stage_name)
                    .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

                stage_config.services.clone()
            } else {
                return Err(anyhow::anyhow!(
                    "ステージ名またはサービス名を指定してください"
                ));
            };

            println!();

            // 複数サービスの場合は色を割り当て
            let colors = vec![
                colored::Color::Cyan,
                colored::Color::Green,
                colored::Color::Yellow,
                colored::Color::Magenta,
                colored::Color::Blue,
            ];

            for (idx, service_name) in target_services.iter().enumerate() {
                let container_name = format!("flow-{}", service_name);
                let service_color = colors[idx % colors.len()];

                if !follow {
                    println!(
                        "{}",
                        format!("=== {} のログ ===", service_name)
                            .bold()
                            .color(service_color)
                    );
                }

                let options = bollard::container::LogsOptions::<String> {
                    follow,
                    stdout: true,
                    stderr: true,
                    tail: lines.to_string(),
                    timestamps: true,
                    ..Default::default()
                };

                use bollard::container::LogOutput;
                use futures_util::stream::StreamExt;

                let mut log_stream = docker.logs(&container_name, Some(options));

                while let Some(log) = log_stream.next().await {
                    match log {
                        Ok(output) => {
                            let prefix = format!("[{}]", service_name).color(service_color);

                            match output {
                                LogOutput::StdOut { message } => {
                                    let msg = String::from_utf8_lossy(&message);
                                    for line in msg.lines() {
                                        if !line.is_empty() {
                                            println!("{} {}", prefix, line);
                                        }
                                    }
                                }
                                LogOutput::StdErr { message } => {
                                    let msg = String::from_utf8_lossy(&message);
                                    for line in msg.lines() {
                                        if !line.is_empty() {
                                            println!("{} {} {}", prefix, "stderr:".red(), line);
                                        }
                                    }
                                }
                                LogOutput::Console { message } => {
                                    let msg = String::from_utf8_lossy(&message);
                                    for line in msg.lines() {
                                        if !line.is_empty() {
                                            println!("{} {}", prefix, line);
                                        }
                                    }
                                }
                                LogOutput::StdIn { .. } => {}
                            }
                        }
                        Err(e) => {
                            eprintln!("  ⚠ ログ取得エラー ({}): {}", service_name, e);
                            break;
                        }
                    }
                }

                if !follow {
                    println!();
                }
            }

            if follow {
                println!();
                println!("{}", "Ctrl+C でログ追跡を終了".dimmed());
            }
        }
        Commands::Validate => {
            println!("{}", "設定を検証中...".blue());

            // プロジェクトルートを検出
            match fleetflow_atom::find_project_root() {
                Ok(project_root) => {
                    println!(
                        "プロジェクトルート: {}",
                        project_root.display().to_string().cyan()
                    );

                    // デバッグモードでロード
                    match fleetflow_atom::load_project_with_debug(&project_root) {
                        Ok(config) => {
                            println!("{}", "✓ 設定ファイルは正常です！".green().bold());
                            println!();
                            println!("サマリー:");
                            println!("  サービス: {}個", config.services.len());
                            for (name, service) in &config.services {
                                let image = service
                                    .image
                                    .as_ref()
                                    .or(service.version.as_ref())
                                    .map(|s| s.as_str())
                                    .unwrap_or("(未設定)");
                                println!("    - {} ({})", name.cyan(), image);
                            }
                            println!("  ステージ: {}個", config.stages.len());
                            for (name, stage) in &config.stages {
                                println!(
                                    "    - {} ({}個のサービス)",
                                    name.cyan(),
                                    stage.services.len()
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!();
                            eprintln!("{}", "✗ 設定エラー".red().bold());
                            eprintln!("  {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!();
                    eprintln!("{}", "✗ プロジェクトルートが見つかりません".red().bold());
                    eprintln!("  {}", e);
                    eprintln!();
                    eprintln!("flow.kdl が存在するディレクトリで実行してください");
                    std::process::exit(1);
                }
            }
        }
        Commands::Version => {
            // すでに上で処理済み
            unreachable!()
        }
    }

    Ok(())
}
