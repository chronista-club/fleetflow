mod tui;

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

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
        println!("unison {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // 設定ファイルを検索
    let config_path = match flow_config::find_flow_file() {
        Ok(path) => path,
        Err(flow_config::ConfigError::FlowFileNotFound) => {
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
    let config = flow_atom::parse_kdl_file(&config_path)?;

    // ここから既存のコマンド処理
    match cli.command {
        Commands::Up { stage } => {
            println!("{}", "ステージを起動中...".green());
            println!("設定ファイル: {}", config_path.display().to_string().cyan());

            // ステージ名の決定
            let stage_name = stage.ok_or_else(|| {
                anyhow::anyhow!(
                    "ステージ名を指定してください: --stage=local または FLOW_STAGE=local"
                )
            })?;

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
            let docker = bollard::Docker::connect_with_local_defaults()?;

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
                    flow_container::service_to_container_config(service_name, service);

                // コンテナ作成
                match docker
                    .create_container(Some(create_options.clone()), container_config.clone())
                    .await
                {
                    Ok(response) => {
                        println!("  ✓ コンテナ作成: {}", response.id);

                        // コンテナ起動
                        docker.start_container::<String>(&response.id, None).await?;
                        println!("  ✓ 起動完了");
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
                            Err(e) => println!("  ⚠ 起動エラー: {}", e),
                        }
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!("コンテナ作成エラー: {}", e));
                    }
                }
            }

            println!();
            println!("{}", "✓ すべてのサービスが起動しました！".green().bold());
        }
        Commands::Down { stage } => {
            println!("{}", "ステージを停止中...".yellow());
            println!("設定ファイル: {}", config_path.display().to_string().cyan());
            if let Some(stage_name) = stage {
                println!("ステージ: {}", stage_name.cyan());
            }
            // TODO: 実装
        }
        Commands::Validate => {
            println!("{}", "設定を検証中...".blue());
            println!("設定ファイル: {}", config_path.display().to_string().cyan());
            // TODO: 実装
        }
        Commands::Version => {
            // すでに上で処理済み
            unreachable!()
        }
    }

    Ok(())
}
