mod tui;

use clap::{Parser, Subcommand};
use colored::Colorize;
use fleetflow_build::{BuildResolver, ContextBuilder, ImageBuilder};
use std::collections::HashMap;
use std::path::PathBuf;

/// Docker config.json からレジストリの認証情報を取得
fn get_docker_credentials(registry: &str) -> Option<bollard::auth::DockerCredentials> {
    // ~/.docker/config.json を読み込み
    let home = std::env::var("HOME").ok()?;
    let config_path = format!("{}/.docker/config.json", home);
    let config_content = std::fs::read_to_string(&config_path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&config_content).ok()?;

    // auths セクションからレジストリの認証情報を取得
    let auths = config.get("auths")?.as_object()?;
    let auth_entry = auths.get(registry)?;
    let auth_b64 = auth_entry.get("auth")?.as_str()?;

    // Base64 デコード (username:password 形式)
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(auth_b64)
        .ok()?;
    let auth_str = String::from_utf8(decoded).ok()?;
    let (username, password) = auth_str.split_once(':')?;

    Some(bollard::auth::DockerCredentials {
        username: Some(username.to_string()),
        password: Some(password.to_string()),
        serveraddress: Some(registry.to_string()),
        ..Default::default()
    })
}

/// イメージ名からレジストリを抽出
fn extract_registry(image: &str) -> Option<&str> {
    // ghcr.io/owner/repo:tag のような形式
    // docker.io/library/nginx:latest のような形式
    // 最初の / の前がレジストリ
    if image.contains('/') {
        let parts: Vec<&str> = image.split('/').collect();
        let first = parts[0];
        // レジストリは . または : を含む（例: ghcr.io, localhost:5000）
        if first.contains('.') || first.contains(':') {
            return Some(first);
        }
    }
    None
}

/// イメージ名とタグを分離
/// 例: "redis:7-alpine" -> ("redis", "7-alpine")
///     "postgres" -> ("postgres", "latest")
fn parse_image_tag(image: &str) -> (&str, &str) {
    if let Some((name, tag)) = image.split_once(':') {
        (name, tag)
    } else {
        (image, "latest")
    }
}

/// ステージ名を決定する（共通ロジック）
fn determine_stage_name(
    stage: Option<String>,
    config: &fleetflow_atom::Flow,
) -> anyhow::Result<String> {
    if let Some(s) = stage {
        Ok(s)
    } else if config.stages.contains_key("default") {
        Ok("default".to_string())
    } else if config.stages.len() == 1 {
        Ok(config.stages.keys().next().unwrap().clone())
    } else {
        Err(anyhow::anyhow!(
            "ステージ名を指定してください: --stage=<stage> または FLOW_STAGE=<stage>\n利用可能なステージ: {}",
            config
                .stages
                .keys()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// 読み込んだ設定ファイル情報を表示
fn print_loaded_config_files(project_root: &std::path::Path) {
    use colored::Colorize;
    println!("📄 読み込んだ設定ファイル:");

    let flow_kdl = project_root.join("flow.kdl");
    if flow_kdl.exists() {
        println!("  • {}", flow_kdl.display().to_string().cyan());
    }

    let flow_local_kdl = project_root.join("flow.local.kdl");
    if flow_local_kdl.exists() {
        println!(
            "  • {} (ローカルオーバーライド)",
            flow_local_kdl.display().to_string().cyan()
        );
    }
}

/// Dockerイメージを自動的にpull
async fn pull_image(docker: &bollard::Docker, image: &str) -> anyhow::Result<()> {
    use futures_util::stream::StreamExt;

    let (image_name, tag) = parse_image_tag(image);

    println!("  ℹ イメージが見つかりません: {}", image.cyan());
    println!("  ↓ イメージをダウンロード中...");

    // レジストリから認証情報を取得（あれば）
    let credentials = extract_registry(image).and_then(get_docker_credentials);

    #[allow(deprecated)]
    let options = bollard::image::CreateImageOptions {
        from_image: image_name,
        tag,
        ..Default::default()
    };

    #[allow(deprecated)]
    let mut stream = docker.create_image(Some(options), None, credentials);

    while let Some(info) = stream.next().await {
        match info {
            Ok(bollard::models::CreateImageInfo {
                status: Some(status),
                progress: Some(progress),
                ..
            }) => {
                // 進捗を表示（同じ行に上書き）
                print!("\r  ↓ {}: {}", status, progress);
                use std::io::Write;
                std::io::stdout().flush()?;
            }
            Ok(bollard::models::CreateImageInfo {
                status: Some(status),
                ..
            }) => {
                // 進捗なしの場合
                print!("\r  ↓ {}                    ", status);
                use std::io::Write;
                std::io::stdout().flush()?;
            }
            Err(e) => {
                println!();
                return Err(anyhow::anyhow!(
                    "イメージのダウンロードに失敗しました: {}",
                    e
                ));
            }
            _ => {}
        }
    }

    println!();
    println!("  ✓ イメージのダウンロード完了");

    Ok(())
}

/// 最新イメージを強制的にpull（--pull フラグ用）
async fn pull_image_always(docker: &bollard::Docker, image: &str) -> anyhow::Result<()> {
    use futures_util::stream::StreamExt;

    let (image_name, tag) = parse_image_tag(image);

    println!("  ↓ 最新イメージをプル中: {}", image.cyan());

    // レジストリから認証情報を取得（あれば）
    let credentials = extract_registry(image).and_then(get_docker_credentials);

    #[allow(deprecated)]
    let options = bollard::image::CreateImageOptions {
        from_image: image_name,
        tag,
        ..Default::default()
    };

    #[allow(deprecated)]
    let mut stream = docker.create_image(Some(options), None, credentials);

    while let Some(info) = stream.next().await {
        match info {
            Ok(bollard::models::CreateImageInfo {
                status: Some(status),
                progress: Some(progress),
                ..
            }) => {
                print!("\r  ↓ {}: {}", status, progress);
                use std::io::Write;
                std::io::stdout().flush()?;
            }
            Ok(bollard::models::CreateImageInfo {
                status: Some(status),
                ..
            }) => {
                print!("\r  ↓ {}                    ", status);
                use std::io::Write;
                std::io::stdout().flush()?;
            }
            Err(e) => {
                println!();
                return Err(anyhow::anyhow!(
                    "イメージのプルに失敗しました: {}",
                    e
                ));
            }
            _ => {}
        }
    }

    println!();
    println!("  ✓ プル完了");

    Ok(())
}

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
        /// 起動前に最新イメージをpullする
        #[arg(short, long)]
        pull: bool,
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
    /// サービスを再起動
    Restart {
        /// サービス名
        service: String,
        /// ステージ名を指定 (local, dev, stg, prd)
        /// 環境変数 FLOW_STAGE からも読み込み可能
        #[arg(short, long, env = "FLOW_STAGE")]
        stage: Option<String>,
    },
    /// サービスを停止
    Stop {
        /// サービス名
        service: String,
        /// ステージ名を指定 (local, dev, stg, prd)
        /// 環境変数 FLOW_STAGE からも読み込み可能
        #[arg(short, long, env = "FLOW_STAGE")]
        stage: Option<String>,
    },
    /// サービスを起動
    Start {
        /// サービス名
        service: String,
        /// ステージ名を指定 (local, dev, stg, prd)
        /// 環境変数 FLOW_STAGE からも読み込み可能
        #[arg(short, long, env = "FLOW_STAGE")]
        stage: Option<String>,
    },
    /// 設定を検証
    Validate,
    /// バージョン情報を表示
    Version,
    /// FleetFlow自体を最新版に更新
    #[command(name = "self-update")]
    SelfUpdate,
    /// ステージをデプロイ（CI/CD向け）
    /// 既存コンテナを強制停止・削除し、最新イメージで再起動
    Deploy {
        /// ステージ名を指定 (local, dev, stg, prd)
        /// 環境変数 FLOW_STAGE からも読み込み可能
        #[arg(short, long, env = "FLOW_STAGE")]
        stage: Option<String>,
        /// 最新イメージを強制的にpull
        #[arg(long)]
        pull: bool,
        /// 確認なしで実行
        #[arg(short, long)]
        yes: bool,
    },
    /// Dockerイメージをビルド
    Build {
        /// ステージ名
        stage: String,
        /// ビルド対象のサービス（省略時は全サービス）
        #[arg(short = 'n', long)]
        service: Option<String>,
        /// ビルド後にレジストリにプッシュ
        #[arg(long)]
        push: bool,
        /// イメージタグを指定（--pushと併用）
        #[arg(long)]
        tag: Option<String>,
        /// キャッシュを使用しない
        #[arg(long)]
        no_cache: bool,
    },
    /// クラウドリソースを管理
    #[command(subcommand)]
    Cloud(CloudCommands),
}

/// クラウド関連のサブコマンド
#[derive(Subcommand)]
enum CloudCommands {
    /// クラウドリソースの状態を表示
    Status {
        /// ステージ名を指定 (production, staging)
        #[arg(short, long)]
        stage: Option<String>,
    },
    /// クラウドプロバイダーの認証状態を確認
    Auth,
    /// クラウドリソースを作成/更新
    Up {
        /// ステージ名を指定
        #[arg(short, long)]
        stage: String,
        /// 確認なしで実行
        #[arg(short, long)]
        yes: bool,
    },
    /// クラウドリソースを削除
    Down {
        /// ステージ名を指定
        #[arg(short, long)]
        stage: String,
        /// 確認なしで実行
        #[arg(short, long)]
        yes: bool,
    },
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

    // SelfUpdateコマンドは設定ファイル不要
    if matches!(cli.command, Commands::SelfUpdate) {
        return self_update().await;
    }

    // プロジェクトルートを検索
    let project_root = match fleetflow_atom::find_project_root() {
        Ok(root) => root,
        Err(fleetflow_atom::FlowError::ProjectRootNotFound(_)) => {
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

    // プロジェクト全体をロード（flow.kdl + flow.local.kdlを自動マージ）
    let config = fleetflow_atom::load_project_from_root(&project_root)?;

    // ここから既存のコマンド処理
    match cli.command {
        Commands::Up { stage, pull } => {
            // 最初にバージョンチェック
            check_and_update_if_needed().await?;

            println!("{}", "ステージを起動中...".green());
            print_loaded_config_files(&project_root);

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

            // ネットワーク作成 (#14)
            let network_name = fleetflow_container::get_network_name(&config.name, &stage_name);
            println!();
            println!("{}", format!("🌐 ネットワーク: {}", network_name).blue());

            let network_config = bollard::models::NetworkCreateRequest {
                name: network_name.clone(),
                driver: Some("bridge".to_string()),
                ..Default::default()
            };

            match docker.create_network(network_config).await {
                Ok(_) => {
                    println!("  ✓ ネットワーク作成完了");
                }
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 409, ..
                }) => {
                    println!("  ℹ ネットワークは既に存在します");
                }
                Err(e) => {
                    eprintln!("  ⚠ ネットワーク作成エラー: {}", e);
                    // ネットワーク作成に失敗しても続行（既存のブリッジネットワークを使用）
                }
            }

            // 各サービスを起動
            for service_name in &stage_config.services {
                let service = config.services.get(service_name).ok_or_else(|| {
                    anyhow::anyhow!("サービス '{}' の定義が見つかりません", service_name)
                })?;

                // imageが設定されているか確認
                if service.image.is_none() {
                    return Err(anyhow::anyhow!(
                        "サービス '{}' に image が指定されていません",
                        service_name
                    ));
                }

                println!();
                println!(
                    "{}",
                    format!("▶ {} を起動中...", service_name).green().bold()
                );

                // サービスをコンテナ設定に変換
                let (container_config, create_options) =
                    fleetflow_container::service_to_container_config(
                        service_name,
                        service,
                        &stage_name,
                        &config.name,
                    );

                // build設定がある場合は先にビルドを実行（ローカルビルド優先）
                if service.build.is_some() {
                    #[allow(deprecated)]
                    let image = container_config
                        .image
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("イメージ名が指定されていません"))?;

                    println!("  🔨 build設定があるためローカルビルドを実行...");

                    let resolver = BuildResolver::new(project_root.to_path_buf());

                    let dockerfile_path = match resolver.resolve_dockerfile(service_name, service) {
                        Ok(Some(path)) => path,
                        Ok(None) => {
                            return Err(anyhow::anyhow!(
                                "Dockerfileが見つかりません: サービス '{}'",
                                service_name
                            ));
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!("Dockerfile解決エラー: {}", e));
                        }
                    };

                    let context_path = match resolver.resolve_context(service) {
                        Ok(path) => path,
                        Err(e) => {
                            return Err(anyhow::anyhow!("コンテキスト解決エラー: {}", e));
                        }
                    };

                    let variables: HashMap<String, String> = std::env::vars().collect();
                    let build_args = resolver.resolve_build_args(service, &variables);
                    let target = service.build.as_ref().and_then(|b| b.target.clone());

                    println!(
                        "  → Dockerfile: {}",
                        dockerfile_path.display().to_string().cyan()
                    );
                    println!("  → Context: {}", context_path.display().to_string().cyan());
                    println!("  → Image: {}", image.cyan());

                    let context_data =
                        match ContextBuilder::create_context(&context_path, &dockerfile_path) {
                            Ok(data) => data,
                            Err(e) => {
                                return Err(anyhow::anyhow!("コンテキスト作成エラー: {}", e));
                            }
                        };

                    let builder = ImageBuilder::new(docker.clone());
                    match builder
                        .build_image(context_data, image, build_args, target.as_deref(), false)
                        .await
                    {
                        Ok(_) => {
                            println!("  {} ビルド完了", "✓".green());
                        }
                        Err(e) => {
                            eprintln!("  ✗ ビルドエラー: {}", e);
                            return Err(anyhow::anyhow!("イメージのビルドに失敗しました"));
                        }
                    }
                }

                // --pull フラグが指定されていて、build設定がない場合は最新イメージをpull
                if pull && service.build.is_none() {
                    #[allow(deprecated)]
                    let image = container_config
                        .image
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("イメージ名が指定されていません"))?;
                    pull_image_always(&docker, image).await?;
                }

                // コンテナ作成
                match docker
                    .create_container(Some(create_options.clone()), container_config.clone())
                    .await
                {
                    Ok(response) => {
                        println!("  ✓ コンテナ作成: {}", response.id);

                        // コンテナ起動
                        match docker
                            .start_container(
                                &response.id,
                                None::<bollard::query_parameters::StartContainerOptions>,
                            )
                            .await
                        {
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
                        #[allow(deprecated)]
                        let container_name = &create_options.name;

                        // 既存コンテナを起動
                        match docker
                            .start_container(
                                container_name,
                                None::<bollard::query_parameters::StartContainerOptions>,
                            )
                            .await
                        {
                            Ok(_) => println!("  ✓ 既存コンテナを起動"),
                            Err(bollard::errors::Error::DockerResponseServerError {
                                status_code: 304,
                                ..
                            }) => {
                                // 既に起動中のコンテナは再起動
                                println!("  ℹ コンテナは既に起動中、再起動します...");
                                match docker
                                    .restart_container(
                                        container_name,
                                        None::<bollard::query_parameters::RestartContainerOptions>,
                                    )
                                    .await
                                {
                                    Ok(_) => println!("  ✓ 再起動完了"),
                                    Err(e) => {
                                        eprintln!("  ✗ 再起動エラー: {}", e);
                                        return Err(anyhow::anyhow!(
                                            "コンテナ再起動に失敗しました"
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("  ✗ 起動エラー: {}", e);
                                return Err(anyhow::anyhow!("コンテナ起動に失敗しました"));
                            }
                        }
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        ..
                    }) => {
                        // イメージが見つからない場合
                        #[allow(deprecated)]
                        let image = container_config
                            .image
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("イメージ名が指定されていません"))?;

                        // build設定があればローカルビルドを優先、なければpull
                        if service.build.is_some() {
                            println!("  ℹ イメージが見つかりません: {}", image.cyan());
                            println!("  🔨 build設定があるためローカルビルドを実行...");

                            // BuildResolver を使ってDockerfileとコンテキストを解決
                            let resolver = BuildResolver::new(project_root.to_path_buf());

                            let dockerfile_path =
                                match resolver.resolve_dockerfile(service_name, service) {
                                    Ok(Some(path)) => path,
                                    Ok(None) => {
                                        return Err(anyhow::anyhow!(
                                            "Dockerfileが見つかりません: サービス '{}'",
                                            service_name
                                        ));
                                    }
                                    Err(e) => {
                                        return Err(anyhow::anyhow!("Dockerfile解決エラー: {}", e));
                                    }
                                };

                            let context_path = match resolver.resolve_context(service) {
                                Ok(path) => path,
                                Err(e) => {
                                    return Err(anyhow::anyhow!("コンテキスト解決エラー: {}", e));
                                }
                            };

                            // ビルド引数を解決
                            let variables: HashMap<String, String> = std::env::vars().collect();
                            let build_args = resolver.resolve_build_args(service, &variables);

                            // ターゲットステージ
                            let target = service.build.as_ref().and_then(|b| b.target.clone());

                            println!(
                                "  → Dockerfile: {}",
                                dockerfile_path.display().to_string().cyan()
                            );
                            println!("  → Context: {}", context_path.display().to_string().cyan());
                            println!("  → Image: {}", image.cyan());

                            // ビルドコンテキストを作成
                            let context_data = match ContextBuilder::create_context(
                                &context_path,
                                &dockerfile_path,
                            ) {
                                Ok(data) => data,
                                Err(e) => {
                                    return Err(anyhow::anyhow!("コンテキスト作成エラー: {}", e));
                                }
                            };

                            // ビルダーを作成してビルド実行
                            let builder = ImageBuilder::new(docker.clone());
                            match builder
                                .build_image(
                                    context_data,
                                    image,
                                    build_args,
                                    target.as_deref(),
                                    false,
                                )
                                .await
                            {
                                Ok(_) => {
                                    println!("  {} ビルド完了", "✓".green());
                                }
                                Err(e) => {
                                    eprintln!("  ✗ ビルドエラー: {}", e);
                                    return Err(anyhow::anyhow!("イメージのビルドに失敗しました"));
                                }
                            }
                        } else {
                            // build設定がない場合はpull
                            pull_image(&docker, image).await?;
                        }

                        // pull成功後、再度コンテナ作成を試行
                        match docker
                            .create_container(
                                Some(create_options.clone()),
                                container_config.clone(),
                            )
                            .await
                        {
                            Ok(response) => {
                                println!("  ✓ コンテナ作成: {}", response.id);

                                // コンテナ起動
                                match docker
                                    .start_container(
                                        &response.id,
                                        None::<bollard::query_parameters::StartContainerOptions>,
                                    )
                                    .await
                                {
                                    Ok(_) => println!("  ✓ 起動完了"),
                                    Err(e) => {
                                        eprintln!("  ✗ 起動エラー: {}", e);
                                        return Err(anyhow::anyhow!("コンテナ起動に失敗しました"));
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("  ✗ コンテナ作成エラー: {}", e);
                                return Err(anyhow::anyhow!("コンテナ作成に失敗しました"));
                            }
                        }
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
            print_loaded_config_files(&project_root);

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

                // OrbStack連携の命名規則を使用: {project}-{stage}-{service}
                let container_name = format!("{}-{}-{}", config.name, stage_name, service_name);

                // コンテナを停止
                match docker
                    .stop_container(
                        &container_name,
                        None::<bollard::query_parameters::StopContainerOptions>,
                    )
                    .await
                {
                    Ok(_) => {
                        println!("  ✓ 停止完了");

                        // --remove フラグが指定されている場合は削除
                        if remove {
                            match docker
                                .remove_container(
                                    &container_name,
                                    None::<bollard::query_parameters::RemoveContainerOptions>,
                                )
                                .await
                            {
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
                            match docker
                                .remove_container(
                                    &container_name,
                                    None::<bollard::query_parameters::RemoveContainerOptions>,
                                )
                                .await
                            {
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

            // ネットワーク削除 (#14)
            if remove {
                let network_name = fleetflow_container::get_network_name(&config.name, &stage_name);
                println!();
                println!(
                    "{}",
                    format!("🌐 ネットワーク削除: {}", network_name).yellow()
                );

                match docker.remove_network(&network_name).await {
                    Ok(_) => {
                        println!("  ✓ ネットワーク削除完了");
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        ..
                    }) => {
                        println!("  ℹ ネットワークは既に存在しません");
                    }
                    Err(e) => {
                        // コンテナがまだ接続されている可能性
                        println!("  ⚠ ネットワーク削除エラー: {}", e);
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
            print_loaded_config_files(&project_root);

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
                // OrbStack連携の命名規則: {project}-{stage}-{service}
                let names: Vec<String> = stage_config
                    .services
                    .iter()
                    .map(|s| format!("{}-{}-{}", config.name, stage_name, s))
                    .collect();
                filter_map.insert("name".to_string(), names);
                Some(filter_map)
            } else {
                // fleetflow.project ラベルでフィルタ
                let mut filter_map = std::collections::HashMap::new();
                filter_map.insert(
                    "label".to_string(),
                    vec![format!("fleetflow.project={}", config.name)],
                );
                Some(filter_map)
            };

            #[allow(deprecated)]
            let options = bollard::container::ListContainersOptions {
                all,
                filters: filters.unwrap_or_default(),
                ..Default::default()
            };

            #[allow(deprecated)]
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
            print_loaded_config_files(&project_root);

            // Docker接続
            let docker = init_docker_with_error_handling().await?;

            // ステージ名を先に取得
            let stage_name = if let Some(ref _service_name) = service {
                // サービス指定の場合でもステージ名が必要
                stage.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Logsコマンドにはステージ名の指定が必要です（-s/--stage）")
                })?
            } else if let Some(ref s) = stage {
                s
            } else {
                return Err(anyhow::anyhow!(
                    "ステージ名を指定してください（-s/--stage）"
                ));
            };

            println!("ステージ: {}", stage_name.cyan());

            // 対象サービスの決定
            let target_services = if let Some(service_name) = service {
                vec![service_name]
            } else {
                let stage_config = config
                    .stages
                    .get(stage_name)
                    .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

                stage_config.services.clone()
            };

            println!();

            // 複数サービスの場合は色を割り当て
            let colors = [
                colored::Color::Cyan,
                colored::Color::Green,
                colored::Color::Yellow,
                colored::Color::Magenta,
                colored::Color::Blue,
            ];

            for (idx, service_name) in target_services.iter().enumerate() {
                // OrbStack連携の命名規則を使用: {project}-{stage}-{service}
                let container_name = format!("{}-{}-{}", config.name, stage_name, service_name);
                let service_color = colors[idx % colors.len()];

                if !follow {
                    println!(
                        "{}",
                        format!("=== {} のログ ===", service_name)
                            .bold()
                            .color(service_color)
                    );
                }

                #[allow(deprecated)]
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
        Commands::Restart { service, stage } => {
            println!(
                "{}",
                format!("サービス '{}' を再起動中...", service).green()
            );

            // ステージ名の決定
            let stage_name = determine_stage_name(stage, &config)?;
            println!("ステージ: {}", stage_name.cyan());

            // サービスの存在確認
            let service_def = config
                .services
                .get(&service)
                .ok_or_else(|| anyhow::anyhow!("サービス '{}' が見つかりません", service))?;

            // Docker接続
            let docker = init_docker_with_error_handling().await?;

            // コンテナ名
            let container_name = format!("{}-{}-{}", config.name, stage_name, service);

            // コンテナの停止
            println!("  ↓ コンテナを停止中...");
            match docker
                .stop_container(
                    &container_name,
                    None::<bollard::query_parameters::StopContainerOptions>,
                )
                .await
            {
                Ok(_) => println!("  ✓ コンテナを停止しました"),
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => {
                    println!("  ℹ コンテナは実行されていません");
                }
                Err(e) => return Err(e.into()),
            }

            // コンテナの起動
            println!("  ↑ コンテナを起動中...");
            match docker
                .start_container(
                    &container_name,
                    None::<bollard::query_parameters::StartContainerOptions>,
                )
                .await
            {
                Ok(_) => {
                    println!("  ✓ コンテナを起動しました");
                    println!();
                    println!(
                        "{}",
                        format!("✓ '{}' を再起動しました", service).green().bold()
                    );
                }
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => {
                    // コンテナが存在しない場合は作成して起動
                    println!("  ℹ コンテナが存在しないため、新規作成します");

                    // コンテナ作成・起動（upコマンドのロジックを再利用）
                    let (container_config, create_options) =
                        fleetflow_container::service_to_container_config(
                            &service,
                            service_def,
                            &stage_name,
                            &config.name,
                        );

                    // イメージ名の取得
                    #[allow(deprecated)]
                    let image = container_config.image.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("サービス '{}' のイメージ設定が見つかりません", service)
                    })?;

                    // イメージの存在確認とpull
                    match docker.inspect_image(image).await {
                        Ok(_) => {}
                        Err(bollard::errors::Error::DockerResponseServerError {
                            status_code: 404,
                            ..
                        }) => {
                            pull_image(&docker, image).await?;
                        }
                        Err(e) => return Err(e.into()),
                    }

                    // コンテナ作成
                    docker
                        .create_container(Some(create_options), container_config)
                        .await?;

                    // コンテナ起動
                    docker
                        .start_container(
                            &container_name,
                            None::<bollard::query_parameters::StartContainerOptions>,
                        )
                        .await?;

                    println!("  ✓ コンテナを作成・起動しました");
                    println!();
                    println!(
                        "{}",
                        format!("✓ '{}' を起動しました", service).green().bold()
                    );
                }
                Err(e) => return Err(e.into()),
            }
        }
        Commands::Stop { service, stage } => {
            println!("{}", format!("サービス '{}' を停止中...", service).yellow());

            // ステージ名の決定
            let stage_name = determine_stage_name(stage, &config)?;
            println!("ステージ: {}", stage_name.cyan());

            // サービスの存在確認
            config
                .services
                .get(&service)
                .ok_or_else(|| anyhow::anyhow!("サービス '{}' が見つかりません", service))?;

            // Docker接続
            let docker = init_docker_with_error_handling().await?;

            // コンテナ名
            let container_name = format!("{}-{}-{}", config.name, stage_name, service);

            // コンテナの停止
            match docker
                .stop_container(
                    &container_name,
                    None::<bollard::query_parameters::StopContainerOptions>,
                )
                .await
            {
                Ok(_) => {
                    println!();
                    println!(
                        "{}",
                        format!("✓ '{}' を停止しました", service).green().bold()
                    );
                }
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => {
                    println!();
                    println!(
                        "{}",
                        format!("ℹ コンテナ '{}' は存在しません", service).dimmed()
                    );
                }
                Err(e) => return Err(e.into()),
            }
        }
        Commands::Start { service, stage } => {
            println!("{}", format!("サービス '{}' を起動中...", service).green());

            // ステージ名の決定
            let stage_name = determine_stage_name(stage, &config)?;
            println!("ステージ: {}", stage_name.cyan());

            // サービスの存在確認
            let service_def = config
                .services
                .get(&service)
                .ok_or_else(|| anyhow::anyhow!("サービス '{}' が見つかりません", service))?;

            // Docker接続
            let docker = init_docker_with_error_handling().await?;

            // コンテナ名
            let container_name = format!("{}-{}-{}", config.name, stage_name, service);

            // コンテナの起動
            match docker
                .start_container(
                    &container_name,
                    None::<bollard::query_parameters::StartContainerOptions>,
                )
                .await
            {
                Ok(_) => {
                    println!();
                    println!(
                        "{}",
                        format!("✓ '{}' を起動しました", service).green().bold()
                    );
                }
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => {
                    // コンテナが存在しない場合は作成して起動
                    println!("  ℹ コンテナが存在しないため、新規作成します");

                    // コンテナ作成・起動（upコマンドのロジックを再利用）
                    let (container_config, create_options) =
                        fleetflow_container::service_to_container_config(
                            &service,
                            service_def,
                            &stage_name,
                            &config.name,
                        );

                    // イメージ名の取得
                    #[allow(deprecated)]
                    let image = container_config.image.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("サービス '{}' のイメージ設定が見つかりません", service)
                    })?;

                    // イメージの存在確認とpull
                    match docker.inspect_image(image).await {
                        Ok(_) => {}
                        Err(bollard::errors::Error::DockerResponseServerError {
                            status_code: 404,
                            ..
                        }) => {
                            pull_image(&docker, image).await?;
                        }
                        Err(e) => return Err(e.into()),
                    }

                    // コンテナ作成
                    docker
                        .create_container(Some(create_options), container_config)
                        .await?;

                    // コンテナ起動
                    docker
                        .start_container(
                            &container_name,
                            None::<bollard::query_parameters::StartContainerOptions>,
                        )
                        .await?;

                    println!("  ✓ コンテナを作成・起動しました");
                    println!();
                    println!(
                        "{}",
                        format!("✓ '{}' を起動しました", service).green().bold()
                    );
                }
                Err(e) => return Err(e.into()),
            }
        }
        Commands::Deploy { stage, pull, yes } => {
            println!("{}", "デプロイを開始します...".blue().bold());
            print_loaded_config_files(&project_root);

            // ステージ名の決定
            let stage_name = determine_stage_name(stage, &config)?;
            println!("ステージ: {}", stage_name.cyan());

            // ステージの取得
            let stage_config = config
                .stages
                .get(&stage_name)
                .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

            println!();
            println!(
                "{}",
                format!("デプロイ対象サービス ({} 個):", stage_config.services.len()).bold()
            );
            for service_name in &stage_config.services {
                let service = config.services.get(service_name);
                let image = service
                    .and_then(|s| s.image.as_ref())
                    .map(|s| s.as_str())
                    .unwrap_or("(イメージ未設定)");
                println!("  • {} ({})", service_name.cyan(), image);
            }

            // 確認（--yesが指定されていない場合）
            if !yes {
                println!();
                println!(
                    "{}",
                    "警告: 既存のコンテナを停止・削除して再作成します。".yellow()
                );
                println!("実行するには --yes オプションを指定してください");
                return Ok(());
            }

            // Docker接続
            println!();
            println!("{}", "Dockerに接続中...".blue());
            let docker = init_docker_with_error_handling().await?;

            // 1. 既存コンテナの停止・削除
            println!();
            println!("{}", "【Step 1/3】既存コンテナを停止・削除中...".yellow());
            for service_name in &stage_config.services {
                let container_name = format!("{}-{}-{}", config.name, stage_name, service_name);

                // 停止
                match docker
                    .stop_container(
                        &container_name,
                        None::<bollard::query_parameters::StopContainerOptions>,
                    )
                    .await
                {
                    Ok(_) => {
                        println!("  ✓ {} を停止しました", service_name.cyan());
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        ..
                    }) => {
                        println!("  - {} (コンテナなし)", service_name);
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 304,
                        ..
                    }) => {
                        println!("  - {} (既に停止中)", service_name);
                    }
                    Err(e) => {
                        println!("  ⚠ {} 停止エラー: {}", service_name, e);
                    }
                }

                // 削除（強制）
                match docker
                    .remove_container(
                        &container_name,
                        Some(bollard::query_parameters::RemoveContainerOptions {
                            force: true,
                            ..Default::default()
                        }),
                    )
                    .await
                {
                    Ok(_) => {
                        println!("  ✓ {} を削除しました", service_name.cyan());
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        ..
                    }) => {
                        // コンテナが存在しない場合は無視
                    }
                    Err(e) => {
                        println!("  ⚠ {} 削除エラー: {}", service_name, e);
                    }
                }
            }

            // 2. イメージのpull（--pullが指定されている場合）
            if pull {
                println!();
                println!("{}", "【Step 2/3】最新イメージをダウンロード中...".blue());
                for service_name in &stage_config.services {
                    if let Some(service) = config.services.get(service_name)
                        && let Some(image) = &service.image
                    {
                        println!("  ↓ {} ({})", service_name.cyan(), image);
                        match pull_image(&docker, image).await {
                            Ok(_) => {}
                            Err(e) => {
                                println!("    ⚠ pullエラー: {}", e);
                            }
                        }
                    }
                }
            } else {
                println!();
                println!("【Step 2/3】イメージpullをスキップ（--pullで強制pull）");
            }

            // 3. コンテナの作成・起動
            println!();
            println!("{}", "【Step 3/3】コンテナを作成・起動中...".green());

            // 依存関係順にソート（簡易版：depends_onがないものを先に）
            let mut ordered_services: Vec<String> = Vec::new();
            let mut remaining: Vec<String> = stage_config.services.clone();

            // まずdepends_onが空のサービスを追加
            remaining.retain(|name| {
                if let Some(service) = config.services.get(name)
                    && service.depends_on.is_empty()
                {
                    ordered_services.push(name.clone());
                    return false;
                }
                true
            });

            // 残りを追加（依存関係があるもの）
            ordered_services.extend(remaining);

            for service_name in &ordered_services {
                let service_def = match config.services.get(service_name) {
                    Some(s) => s,
                    None => {
                        println!("  ⚠ サービス '{}' の定義が見つかりません", service_name);
                        continue;
                    }
                };

                println!();
                println!(
                    "{}",
                    format!("■ {} を起動中...", service_name).green().bold()
                );

                let (container_config, create_options) =
                    fleetflow_container::service_to_container_config(
                        service_name,
                        service_def,
                        &stage_name,
                        &config.name,
                    );

                // イメージ確認
                #[allow(deprecated)]
                let image = container_config.image.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("サービス '{}' のイメージ設定が見つかりません", service_name)
                })?;

                // イメージの存在確認（pullしていない場合）
                if !pull {
                    match docker.inspect_image(image).await {
                        Ok(_) => {}
                        Err(bollard::errors::Error::DockerResponseServerError {
                            status_code: 404,
                            ..
                        }) => {
                            pull_image(&docker, image).await?;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }

                // コンテナ作成
                match docker
                    .create_container(Some(create_options.clone()), container_config.clone())
                    .await
                {
                    Ok(_) => {
                        println!("  ✓ コンテナを作成しました");
                    }
                    Err(e) => {
                        println!("  ✗ コンテナ作成エラー: {}", e);
                        return Err(e.into());
                    }
                }

                // 依存サービスの待機（wait_forが設定されている場合）
                if let Some(wait_config) = &service_def.wait_for
                    && !service_def.depends_on.is_empty()
                {
                    println!("  ↻ 依存サービスの準備完了を待機中...");
                    for dep_service in &service_def.depends_on {
                        let dep_container =
                            format!("{}-{}-{}", config.name, stage_name, dep_service);
                        match fleetflow_container::wait_for_service(
                            &docker,
                            &dep_container,
                            wait_config,
                        )
                        .await
                        {
                            Ok(_) => {
                                println!("    ✓ {} が準備完了", dep_service.cyan());
                            }
                            Err(e) => {
                                println!("    ⚠ {} の待機でエラー: {}", dep_service.yellow(), e);
                            }
                        }
                    }
                }

                // コンテナ起動
                let container_name = format!("{}-{}-{}", config.name, stage_name, service_name);
                match docker
                    .start_container(
                        &container_name,
                        None::<bollard::query_parameters::StartContainerOptions>,
                    )
                    .await
                {
                    Ok(_) => {
                        println!("  ✓ 起動完了");
                    }
                    Err(e) => {
                        println!("  ✗ 起動エラー: {}", e);
                        return Err(e.into());
                    }
                }
            }

            println!();
            println!(
                "{}",
                format!("✓ デプロイ完了: ステージ '{}'", stage_name)
                    .green()
                    .bold()
            );
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
                                let server_info = if stage.servers.is_empty() {
                                    String::new()
                                } else {
                                    format!(", {}個のサーバー", stage.servers.len())
                                };
                                println!(
                                    "    - {} ({}個のサービス{})",
                                    name.cyan(),
                                    stage.services.len(),
                                    server_info
                                );
                            }

                            // クラウドリソースの表示
                            if !config.providers.is_empty() {
                                println!("  プロバイダー: {}個", config.providers.len());
                                for (name, provider) in &config.providers {
                                    let zone = provider.zone.as_deref().unwrap_or("(未設定)");
                                    println!("    - {} (zone: {})", name.cyan(), zone);
                                }
                            }
                            if !config.servers.is_empty() {
                                println!("  サーバー: {}個", config.servers.len());
                                for (name, server) in &config.servers {
                                    println!("    - {} ({})", name.cyan(), server.provider);
                                }
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
        Commands::Build {
            stage,
            service,
            push,
            tag,
            no_cache,
        } => {
            handle_build_command(
                &project_root,
                &config,
                &stage,
                service.as_deref(),
                push,
                tag.as_deref(),
                no_cache,
            )
            .await?;
        }
        Commands::Cloud(cloud_cmd) => {
            handle_cloud_command(cloud_cmd, &config).await?;
        }
        Commands::SelfUpdate => {
            // 早期リターンで処理済み（main関数冒頭）
            unreachable!("SelfUpdate is handled before config loading");
        }
    }

    Ok(())
}

/// クラウドコマンドを処理
async fn handle_cloud_command(
    cmd: CloudCommands,
    config: &fleetflow_atom::Flow,
) -> anyhow::Result<()> {
    use fleetflow_cloud::CloudProvider;
    use fleetflow_cloud_cloudflare::{CloudflareDns, DnsConfig};
    use fleetflow_cloud_sakura::SakuraCloudProvider;

    match cmd {
        CloudCommands::Auth => {
            println!("{}", "クラウドプロバイダーの認証状態を確認中...".blue());

            for (name, provider_config) in &config.providers {
                println!("\n{} {}:", "Provider:".cyan(), name.bold());

                // 現在はsakura-cloudのみサポート
                if name == "sakura-cloud" {
                    let zone = provider_config.zone.as_deref().unwrap_or("tk1a");
                    let provider = SakuraCloudProvider::new(zone);

                    match provider.check_auth().await {
                        Ok(status) => {
                            if status.authenticated {
                                println!("  {} 認証済み", "✓".green().bold());
                                if let Some(info) = status.account_info {
                                    println!("  アカウント: {}", info.cyan());
                                }
                            } else {
                                println!("  {} 未認証", "✗".red().bold());
                                if let Some(err) = status.error {
                                    println!("  エラー: {}", err);
                                }
                            }
                        }
                        Err(e) => {
                            println!("  {} 認証チェック失敗: {}", "✗".red().bold(), e);
                        }
                    }
                } else {
                    println!(
                        "  {} プロバイダー '{}' はサポートされていません",
                        "!".yellow(),
                        name
                    );
                }
            }

            if config.providers.is_empty() {
                println!("{}", "クラウドプロバイダーが設定されていません。".yellow());
                println!("flow.kdl に provider ブロックを追加してください。");
            }
        }
        CloudCommands::Status { stage } => {
            println!("{}", "クラウドリソースの状態を取得中...".blue());

            // ステージ名が指定されていない場合は全サーバーを表示
            let servers_to_show: Vec<&str> = if let Some(ref stage_name) = stage {
                if let Some(stage_config) = config.stages.get(stage_name) {
                    stage_config.servers.iter().map(|s| s.as_str()).collect()
                } else {
                    println!(
                        "{} ステージ '{}' が見つかりません",
                        "✗".red().bold(),
                        stage_name
                    );
                    return Ok(());
                }
            } else {
                config.servers.keys().map(|s| s.as_str()).collect()
            };

            if servers_to_show.is_empty() {
                println!("{}", "サーバーリソースが設定されていません。".yellow());
                return Ok(());
            }

            println!("\n{}", "サーバーリソース:".bold());
            for server_name in servers_to_show {
                if let Some(server) = config.servers.get(server_name) {
                    println!("  {} {}", "•".cyan(), server_name.bold());
                    println!("    プロバイダー: {}", server.provider.cyan());
                    if let Some(plan) = &server.plan {
                        println!("    プラン: {}", plan);
                    }
                    if let Some(disk) = server.disk_size {
                        println!("    ディスク: {}GB", disk);
                    }
                }
            }
        }
        CloudCommands::Up { stage, yes } => {
            println!(
                "{}",
                format!("ステージ '{}' のクラウドリソースを作成中...", stage).blue()
            );

            let stage_config = config
                .stages
                .get(&stage)
                .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage))?;

            if stage_config.servers.is_empty() {
                println!(
                    "{}",
                    "このステージにはサーバーリソースがありません。".yellow()
                );
                return Ok(());
            }

            if !yes {
                println!("\n以下のサーバーを作成します:");
                for server_name in &stage_config.servers {
                    if let Some(server) = config.servers.get(server_name) {
                        println!("  - {} ({})", server_name.cyan(), server.provider);
                    }
                }
                println!("\n実行するには --yes オプションを指定してください");
                return Ok(());
            }

            // 各サーバーを作成
            for server_name in &stage_config.servers {
                let server = config.servers.get(server_name).ok_or_else(|| {
                    anyhow::anyhow!("サーバー '{}' の定義が見つかりません", server_name)
                })?;

                println!("\n{} {} を処理中...", "▶".cyan(), server_name.bold());

                // プロバイダー別の処理
                if server.provider == "sakura-cloud" {
                    // プロバイダー設定からzoneを取得
                    let zone = config
                        .providers
                        .get("sakura-cloud")
                        .and_then(|p| p.zone.as_deref())
                        .unwrap_or("tk1a");

                    let provider = SakuraCloudProvider::new(zone);

                    // タグベースの冪等性チェック
                    println!("  ↓ 既存サーバーを検索中...");
                    match provider.find_server_by_tag(&config.name, server_name).await {
                        Ok(Some(existing)) => {
                            println!("  {} サーバーは既に存在します", "✓".green().bold());
                            println!("    ID: {}", existing.id.cyan());
                            println!(
                                "    状態: {}",
                                if existing.is_running {
                                    "起動中".green()
                                } else {
                                    "停止中".yellow()
                                }
                            );
                            if let Some(ip) = &existing.ip_address {
                                println!("    IP: {}", ip.cyan());

                                // 既存サーバーでもDNS設定を確認・更新
                                if let Ok(dns_config) = DnsConfig::from_env() {
                                    let dns = CloudflareDns::new(dns_config);
                                    let subdomain = dns.generate_subdomain(server_name, &stage);
                                    match dns.ensure_record(&subdomain, ip).await {
                                        Ok(record) => {
                                            println!(
                                                "    {} DNS: {}",
                                                "✓".green().bold(),
                                                record.name.cyan()
                                            );

                                            // DNSエイリアス（CNAME）の設定
                                            if !server.dns_aliases.is_empty() {
                                                println!("    ↓ DNSエイリアスを確認・設定中...");
                                                for alias in &server.dns_aliases {
                                                    let target = dns.full_domain(&subdomain);
                                                    match dns
                                                        .ensure_cname_record(alias, &target)
                                                        .await
                                                    {
                                                        Ok(cname_record) => {
                                                            println!(
                                                                "      {} CNAME: {} -> {}",
                                                                "✓".green().bold(),
                                                                cname_record.name.cyan(),
                                                                target.dimmed()
                                                            );
                                                        }
                                                        Err(e) => {
                                                            println!(
                                                                "      {} CNAME設定エラー ({}): {}",
                                                                "⚠".yellow(),
                                                                alias,
                                                                e
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            println!("    {} DNS設定エラー: {}", "⚠".yellow(), e);
                                        }
                                    }
                                }
                            }
                            continue;
                        }
                        Ok(None) => {
                            println!("  ℹ 既存サーバーなし、新規作成します");
                        }
                        Err(e) => {
                            println!("  {} サーバー検索エラー: {}", "✗".red().bold(), e);
                            continue;
                        }
                    }

                    // 新規作成
                    println!("  ↓ サーバーを作成中...");
                    let create_config = fleetflow_cloud_sakura::CreateServerOptions {
                        name: server_name.clone(),
                        plan: server.plan.clone(),
                        disk_size: server.disk_size.map(|d| d as i32),
                        os: server.os.clone(),
                        ssh_keys: server.ssh_keys.clone(),
                        startup_scripts: server.startup_script.clone().into_iter().collect(),
                        tags: vec![
                            format!("fleetflow:{}:{}", config.name, server_name),
                            format!("fleetflow:project:{}", config.name),
                        ],
                    };

                    match provider.create_server(&create_config).await {
                        Ok(info) => {
                            println!("  {} サーバー作成完了!", "✓".green().bold());
                            println!("    ID: {}", info.id.cyan());
                            if let Some(ip) = &info.ip_address {
                                println!("    IP: {}", ip.cyan());

                                // DNS設定（環境変数が設定されている場合）
                                if let Ok(dns_config) = DnsConfig::from_env() {
                                    let dns = CloudflareDns::new(dns_config);
                                    let subdomain = dns.generate_subdomain(server_name, &stage);
                                    println!("  ↓ DNSレコードを設定中...");
                                    match dns.ensure_record(&subdomain, ip).await {
                                        Ok(record) => {
                                            println!(
                                                "  {} DNS: {}",
                                                "✓".green().bold(),
                                                record.name.cyan()
                                            );

                                            // DNSエイリアス（CNAME）の設定
                                            if !server.dns_aliases.is_empty() {
                                                println!("  ↓ DNSエイリアスを設定中...");
                                                for alias in &server.dns_aliases {
                                                    // CNAMEのターゲットは server-stage.domain の形式
                                                    let target = dns.full_domain(&subdomain);
                                                    match dns
                                                        .ensure_cname_record(alias, &target)
                                                        .await
                                                    {
                                                        Ok(cname_record) => {
                                                            println!(
                                                                "    {} CNAME: {} -> {}",
                                                                "✓".green().bold(),
                                                                cname_record.name.cyan(),
                                                                target.dimmed()
                                                            );
                                                        }
                                                        Err(e) => {
                                                            println!(
                                                                "    {} CNAME設定エラー ({}): {}",
                                                                "⚠".yellow(),
                                                                alias,
                                                                e
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            println!("  {} DNS設定エラー: {}", "⚠".yellow(), e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            println!("  {} サーバー作成エラー: {}", "✗".red().bold(), e);
                        }
                    }
                } else {
                    println!(
                        "  {} プロバイダー '{}' はサポートされていません",
                        "!".yellow(),
                        server.provider
                    );
                }
            }

            println!(
                "\n{}",
                "✓ クラウドリソースの処理が完了しました".green().bold()
            );
        }
        CloudCommands::Down { stage, yes } => {
            println!(
                "{}",
                format!("ステージ '{}' のクラウドリソースを削除中...", stage).blue()
            );

            let stage_config = config
                .stages
                .get(&stage)
                .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage))?;

            if stage_config.servers.is_empty() {
                println!(
                    "{}",
                    "このステージにはサーバーリソースがありません。".yellow()
                );
                return Ok(());
            }

            if !yes {
                println!("\n{}", "警告: 以下のサーバーを削除します:".red().bold());
                for server_name in &stage_config.servers {
                    if let Some(server) = config.servers.get(server_name) {
                        println!("  - {} ({})", server_name.cyan(), server.provider);
                    }
                }
                println!("\n実行するには --yes オプションを指定してください");
                return Ok(());
            }

            // 各サーバーを削除
            for server_name in &stage_config.servers {
                let server = config.servers.get(server_name).ok_or_else(|| {
                    anyhow::anyhow!("サーバー '{}' の定義が見つかりません", server_name)
                })?;

                println!("\n{} {} を削除中...", "▶".cyan(), server_name.bold());

                // プロバイダー別の処理
                if server.provider == "sakura-cloud" {
                    // プロバイダー設定からzoneを取得
                    let zone = config
                        .providers
                        .get("sakura-cloud")
                        .and_then(|p| p.zone.as_deref())
                        .unwrap_or("tk1a");

                    let provider = SakuraCloudProvider::new(zone);

                    // タグでサーバーを検索
                    println!("  ↓ サーバーを検索中...");
                    match provider.find_server_by_tag(&config.name, server_name).await {
                        Ok(Some(existing)) => {
                            println!(
                                "  ℹ サーバー発見: {} (ID: {})",
                                server_name,
                                existing.id.cyan()
                            );

                            // DNS削除（環境変数が設定されている場合）
                            if let Ok(dns_config) = DnsConfig::from_env() {
                                let dns = CloudflareDns::new(dns_config);

                                // DNSエイリアス（CNAME）の削除
                                if !server.dns_aliases.is_empty() {
                                    println!("  ↓ DNSエイリアスを削除中...");
                                    for alias in &server.dns_aliases {
                                        match dns.remove_cname_record(alias).await {
                                            Ok(_) => {
                                                println!(
                                                    "    {} CNAME削除: {}.{}",
                                                    "✓".green().bold(),
                                                    alias,
                                                    dns.domain()
                                                );
                                            }
                                            Err(e) => {
                                                println!(
                                                    "    {} CNAME削除エラー ({}): {}",
                                                    "⚠".yellow(),
                                                    alias,
                                                    e
                                                );
                                            }
                                        }
                                    }
                                }

                                // メインのAレコードを削除
                                let subdomain = dns.generate_subdomain(server_name, &stage);
                                println!("  ↓ DNSレコードを削除中...");
                                match dns.remove_record(&subdomain).await {
                                    Ok(_) => {
                                        println!(
                                            "  {} DNS削除: {}.{}",
                                            "✓".green().bold(),
                                            subdomain,
                                            dns.domain()
                                        );
                                    }
                                    Err(e) => {
                                        println!("  {} DNS削除エラー: {}", "⚠".yellow(), e);
                                    }
                                }
                            }

                            // 削除実行
                            println!("  ↓ サーバーを削除中（ディスク含む）...");
                            match provider.delete_server(&existing.id, true).await {
                                Ok(_) => {
                                    println!("  {} サーバー削除完了!", "✓".green().bold());
                                }
                                Err(e) => {
                                    println!("  {} サーバー削除エラー: {}", "✗".red().bold(), e);
                                }
                            }
                        }
                        Ok(None) => {
                            println!(
                                "  {} サーバーが見つかりません（既に削除済み？）",
                                "ℹ".yellow()
                            );
                        }
                        Err(e) => {
                            println!("  {} サーバー検索エラー: {}", "✗".red().bold(), e);
                        }
                    }
                } else {
                    println!(
                        "  {} プロバイダー '{}' はサポートされていません",
                        "!".yellow(),
                        server.provider
                    );
                }
            }

            println!(
                "\n{}",
                "✓ クラウドリソースの削除処理が完了しました".green().bold()
            );
        }
    }

    Ok(())
}

/// ビルドコマンドを処理
async fn handle_build_command(
    project_root: &std::path::Path,
    config: &fleetflow_atom::Flow,
    stage_name: &str,
    service_filter: Option<&str>,
    push: bool,
    cli_tag: Option<&str>,
    no_cache: bool,
) -> anyhow::Result<()> {
    use fleetflow_build::{BuildResolver, ContextBuilder, ImageBuilder, ImagePusher, resolve_tag};
    use std::collections::HashMap;

    println!("{}", "Dockerイメージをビルド中...".green());
    print_loaded_config_files(project_root);
    println!("ステージ: {}", stage_name.cyan());

    // ステージの取得
    let stage_config = config
        .stages
        .get(stage_name)
        .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

    // ビルド対象のサービスを決定
    let target_services: Vec<&String> = if let Some(filter) = service_filter {
        // 特定のサービスのみ
        if !stage_config.services.contains(&filter.to_string()) {
            return Err(anyhow::anyhow!(
                "サービス '{}' はステージ '{}' に含まれていません",
                filter,
                stage_name
            ));
        }
        stage_config
            .services
            .iter()
            .filter(|s| *s == filter)
            .collect()
    } else {
        // 全サービス
        stage_config.services.iter().collect()
    };

    // ビルド可能なサービスをフィルタ（build設定があるもののみ）
    let buildable_services: Vec<(&String, &fleetflow_atom::Service)> = target_services
        .iter()
        .filter_map(|service_name| {
            config.services.get(*service_name).and_then(|service| {
                // build設定があるサービスのみビルド対象
                if service.build.is_some() {
                    Some((*service_name, service))
                } else {
                    None
                }
            })
        })
        .collect();

    if buildable_services.is_empty() {
        println!(
            "{}",
            "ビルド対象のサービスがありません（build 設定が必要です）".yellow()
        );
        return Ok(());
    }

    println!();
    println!(
        "{}",
        format!("ビルド対象サービス ({} 個):", buildable_services.len()).bold()
    );
    for (name, _) in &buildable_services {
        println!("  • {}", name.cyan());
    }

    // Docker接続
    println!();
    println!("{}", "Dockerに接続中...".blue());
    let docker = init_docker_with_error_handling().await?;

    // BuildResolver と ImageBuilder を作成
    let resolver = BuildResolver::new(project_root.to_path_buf());
    let builder = ImageBuilder::new(docker.clone());

    // プッシュが必要な場合は ImagePusher も作成
    let pusher = if push {
        Some(ImagePusher::new(docker.clone()))
    } else {
        None
    };

    // ビルド結果を格納
    let mut build_results: Vec<(String, String)> = Vec::new();

    // 各サービスをビルド
    for (service_name, service) in &buildable_services {
        println!();
        println!(
            "{}",
            format!("🔨 {} をビルド中...", service_name).green().bold()
        );

        // Dockerfileを解決
        let dockerfile_path = match resolver.resolve_dockerfile(service_name, service) {
            Ok(Some(path)) => path,
            Ok(None) => {
                println!(
                    "  {} Dockerfileが見つかりません。スキップします。",
                    "⚠".yellow()
                );
                continue;
            }
            Err(e) => {
                eprintln!("  {} Dockerfile解決エラー: {}", "✗".red().bold(), e);
                return Err(anyhow::anyhow!("Dockerfile解決に失敗しました"));
            }
        };

        // コンテキストを解決
        let context_path = match resolver.resolve_context(service) {
            Ok(path) => path,
            Err(e) => {
                eprintln!("  {} コンテキスト解決エラー: {}", "✗".red().bold(), e);
                return Err(anyhow::anyhow!("コンテキスト解決に失敗しました"));
            }
        };

        // イメージタグを解決
        let image_name = service.image.as_deref().unwrap_or(service_name.as_str());
        let (base_image, tag) = resolve_tag(cli_tag, image_name);
        let full_image = format!("{}:{}", base_image, tag);

        // ビルド引数を解決
        let variables: HashMap<String, String> = std::env::vars().collect();
        let build_args = resolver.resolve_build_args(service, &variables);

        // ターゲットステージ
        let target = service.build.as_ref().and_then(|b| b.target.clone());

        println!(
            "  → Dockerfile: {}",
            dockerfile_path.display().to_string().cyan()
        );
        println!("  → Context: {}", context_path.display().to_string().cyan());
        println!("  → Image: {}", full_image.cyan());

        // ビルドコンテキストを作成
        let context_data = match ContextBuilder::create_context(&context_path, &dockerfile_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("  {} コンテキスト作成エラー: {}", "✗".red().bold(), e);
                return Err(anyhow::anyhow!("ビルドコンテキストの作成に失敗しました"));
            }
        };

        // ビルド実行
        match builder
            .build_image(
                context_data,
                &full_image,
                build_args,
                target.as_deref(),
                no_cache,
            )
            .await
        {
            Ok(_) => {
                println!("  {} ビルド完了", "✓".green());
                build_results.push((service_name.to_string(), full_image));
            }
            Err(e) => {
                eprintln!("  {} ビルドエラー: {}", "✗".red().bold(), e);
                return Err(anyhow::anyhow!("ビルドに失敗しました"));
            }
        }
    }

    // プッシュ処理
    if let Some(pusher) = pusher {
        println!();
        println!("{}", "📤 イメージをプッシュ中...".blue().bold());

        for (service_name, full_image) in &build_results {
            println!();
            println!("{}", format!("Pushing {}...", service_name).blue());

            // イメージとタグを分離
            let (image, tag) = fleetflow_build::split_image_tag(full_image);

            match pusher.push(&image, &tag).await {
                Ok(pushed_image) => {
                    println!("  {} {}", "✓".green(), pushed_image.cyan());
                }
                Err(e) => {
                    eprintln!("  {} プッシュエラー: {}", "✗".red().bold(), e);
                    return Err(anyhow::anyhow!("プッシュに失敗しました"));
                }
            }
        }
    }

    // 完了メッセージ
    println!();
    if push {
        println!(
            "{}",
            "✓ すべてのイメージがビルド＆プッシュされました！"
                .green()
                .bold()
        );
    } else {
        println!(
            "{}",
            "✓ すべてのイメージがビルドされました！".green().bold()
        );
    }

    // 結果サマリー
    println!();
    println!("{}", "結果サマリー:".bold());
    for (service_name, full_image) in &build_results {
        println!("  {} {}: {}", "✓".green(), service_name, full_image.cyan());
    }

    Ok(())
}

/// FleetFlow self-update: GitHub Releasesから最新版をダウンロードして更新
async fn self_update() -> anyhow::Result<()> {
    use std::process::Command;

    println!("{}", "🔄 FleetFlow self-update".blue().bold());
    println!();

    let current_version = env!("CARGO_PKG_VERSION");
    println!("現在のバージョン: {}", current_version.cyan());

    // GitHub APIから最新リリース情報を取得
    println!("最新バージョンを確認中...");

    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/repos/chronista-club/fleetflow/releases/latest")
        .header("User-Agent", "fleetflow-cli")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "GitHubからリリース情報を取得できませんでした: {}",
            response.status()
        ));
    }

    let release: serde_json::Value = response.json().await?;
    let latest_version = release["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("tag_nameが見つかりません"))?
        .trim_start_matches('v');

    println!("最新バージョン: {}", latest_version.green());

    // バージョン比較
    if current_version == latest_version {
        println!();
        println!("{}", "✓ 既に最新版です！".green().bold());
        return Ok(());
    }

    println!();
    println!(
        "{}",
        format!("新しいバージョン {} が利用可能です", latest_version).yellow()
    );

    // ダウンロードURL決定
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let asset_name = match (os, arch) {
        ("macos", "aarch64") => "fleetflow-darwin-arm64.tar.gz",
        ("macos", "x86_64") => "fleetflow-darwin-amd64.tar.gz",
        ("linux", "x86_64") => "fleetflow-linux-amd64.tar.gz",
        ("linux", "aarch64") => "fleetflow-linux-arm64.tar.gz",
        _ => {
            return Err(anyhow::anyhow!(
                "このプラットフォームはサポートされていません: {}-{}",
                os,
                arch
            ));
        }
    };

    // ダウンロードURLを取得
    let assets = release["assets"].as_array();

    let download_url = assets.and_then(|arr| {
        arr.iter()
            .find(|a| a["name"].as_str() == Some(asset_name))
            .and_then(|a| a["browser_download_url"].as_str())
    });

    // バイナリがない場合は cargo install を使用
    let download_url = match download_url {
        Some(url) => url.to_string(),
        None => {
            println!(
                "{}",
                format!("プリビルドバイナリが見つかりません（{}）", asset_name).yellow()
            );
            println!("cargo install でビルドします...");
            println!();

            return cargo_install_update().await;
        }
    };

    println!("ダウンロード中: {}", asset_name);

    // 一時ディレクトリにダウンロード
    let temp_dir = std::env::temp_dir().join("fleetflow-update");
    std::fs::create_dir_all(&temp_dir)?;

    let tar_path = temp_dir.join(asset_name);

    // ダウンロード
    let response = client.get(&download_url).send().await?;
    let bytes = response.bytes().await?;
    std::fs::write(&tar_path, &bytes)?;

    println!("展開中...");

    // tar.gzを展開
    let output = Command::new("tar")
        .args([
            "-xzf",
            tar_path.to_str().unwrap(),
            "-C",
            temp_dir.to_str().unwrap(),
        ])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "展開に失敗しました: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // 現在のバイナリパスを取得
    let current_exe = std::env::current_exe()?;
    let new_binary = temp_dir.join("fleetflow");

    // バイナリを置換
    println!("インストール中...");

    // まず古いバイナリをリネーム（バックアップ）
    let backup_path = current_exe.with_extension("old");
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)?;
    }

    // 新しいバイナリをコピー
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // 実行権限を付与
        let mut perms = std::fs::metadata(&new_binary)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&new_binary, perms)?;
    }

    // self-replaceを使う代わりに、直接コピー
    // (実行中のバイナリは上書きできないため、/usr/local/bin等にインストールされている場合はsudo必要)
    match std::fs::copy(&new_binary, &current_exe) {
        Ok(_) => {
            println!();
            println!(
                "{}",
                format!("✓ FleetFlow {} に更新しました！", latest_version)
                    .green()
                    .bold()
            );
        }
        Err(e) if e.raw_os_error() == Some(26) || e.raw_os_error() == Some(1) => {
            // Text file busy (26) or Permission denied (1)
            println!();
            println!("{}", "⚠ 実行中のバイナリを直接置換できません。".yellow());
            println!("以下のコマンドを実行してください:");
            println!();
            println!(
                "  sudo cp {} {}",
                new_binary.display(),
                current_exe.display()
            );
        }
        Err(e) => return Err(e.into()),
    }

    // クリーンアップ
    std::fs::remove_dir_all(&temp_dir).ok();

    Ok(())
}

/// 起動時にバージョンチェックを行い、更新があれば通知・更新
async fn check_and_update_if_needed() -> anyhow::Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");

    // GitHub APIから最新リリース情報を取得（タイムアウト短め）
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let response = match client
        .get("https://api.github.com/repos/chronista-club/fleetflow/releases/latest")
        .header("User-Agent", "fleetflow-cli")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => {
            // ネットワークエラーは無視して続行
            return Ok(());
        }
    };

    if !response.status().is_success() {
        // APIエラーは無視して続行
        return Ok(());
    }

    let release: serde_json::Value = match response.json().await {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };

    let latest_version = match release["tag_name"].as_str() {
        Some(tag) => tag.trim_start_matches('v'),
        None => return Ok(()),
    };

    // バージョン比較
    if is_newer_version(latest_version, current_version) {
        println!();
        println!(
            "📦 新しいバージョン {} が利用可能です（現在: {}）",
            latest_version.green(),
            current_version.yellow()
        );
        println!("{}", "   更新するには: fleetflow self-update".dimmed());
        println!();

        // 自動更新の確認
        print!("今すぐ更新しますか？ [y/N]: ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if input.trim().eq_ignore_ascii_case("y") {
            return self_update().await;
        }
        println!();
    }

    Ok(())
}

/// バージョン比較: new_ver が current_ver より新しければ true
fn is_newer_version(new_ver: &str, current_ver: &str) -> bool {
    let parse_version =
        |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse().ok()).collect() };

    let new_parts = parse_version(new_ver);
    let current_parts = parse_version(current_ver);

    for (n, c) in new_parts.iter().zip(current_parts.iter()) {
        if n > c {
            return true;
        }
        if n < c {
            return false;
        }
    }

    // 桁数が多い方が新しい (例: 1.0.1 > 1.0)
    new_parts.len() > current_parts.len()
}

/// cargo install でFleetFlowを更新
async fn cargo_install_update() -> anyhow::Result<()> {
    use std::process::Command;

    println!(
        "{}",
        "🔧 cargo install --git https://github.com/chronista-club/fleetflow --force".cyan()
    );
    println!();

    let status = Command::new("cargo")
        .args([
            "install",
            "--git",
            "https://github.com/chronista-club/fleetflow",
            "--force",
        ])
        .status()?;

    if status.success() {
        println!();
        println!("{}", "✓ FleetFlow を更新しました！".green().bold());
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "cargo install に失敗しました（終了コード: {:?}）",
            status.code()
        ))
    }
}
