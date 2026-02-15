use crate::docker;
use crate::utils;
use colored::Colorize;
use std::collections::HashMap;

/// docker buildx を使用したクロスプラットフォームビルド
#[allow(clippy::too_many_arguments)]
async fn build_with_buildx(
    dockerfile_path: &std::path::Path,
    context_path: &std::path::Path,
    image_tag: &str,
    platform: &str,
    build_args: &HashMap<String, String>,
    target: Option<&str>,
    no_cache: bool,
    push: bool,
) -> anyhow::Result<()> {
    use std::process::Command;

    println!("  {} docker buildx build を実行中...", "→".blue());

    let mut cmd = Command::new("docker");
    cmd.arg("buildx")
        .arg("build")
        .arg("--platform")
        .arg(platform)
        .arg("-t")
        .arg(image_tag)
        .arg("-f")
        .arg(dockerfile_path);

    // ビルド引数を追加
    for (key, value) in build_args {
        cmd.arg("--build-arg").arg(format!("{}={}", key, value));
    }

    // ターゲットステージ
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }

    // キャッシュなし
    if no_cache {
        cmd.arg("--no-cache");
    }

    // プッシュフラグ
    if push {
        cmd.arg("--push");
    } else {
        // プッシュしない場合はローカルにロード
        cmd.arg("--load");
    }

    // コンテキストパス
    cmd.arg(context_path);

    // コマンド実行
    let output = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("docker buildxの実行に失敗しました: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("docker buildx build 失敗:\n{}", stderr));
    }

    Ok(())
}

/// ビルドコマンドを処理
#[allow(clippy::too_many_arguments)]
pub async fn handle_build_command(
    project_root: &std::path::Path,
    config: &fleetflow_core::Flow,
    stage_name: &str,
    service_filter: Option<&str>,
    push: bool,
    cli_tag: Option<&str>,
    registry: Option<&str>,
    platform: Option<&str>,
    no_cache: bool,
) -> anyhow::Result<()> {
    use fleetflow_build::{BuildResolver, ImageBuilder, ImagePusher, resolve_tag};

    // ステージの取得
    let stage_config = config
        .stages
        .get(stage_name)
        .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

    // localステージ以外はクロスプラットフォームビルドを使用
    // registry優先順位: CLI > Stage > Flow（Service levelは後で個別に確認）
    let is_local = stage_name == "local";
    let has_config_registry =
        registry.is_some() || stage_config.registry.is_some() || config.registry.is_some();
    let use_buildx = !is_local && (platform.is_some() || has_config_registry || push);
    let target_platform = platform.unwrap_or(if is_local { "" } else { "linux/amd64" });

    println!("{}", "Dockerイメージをビルド中...".green());
    utils::print_loaded_config_files(project_root);
    println!("ステージ: {}", stage_name.cyan());
    if use_buildx && !target_platform.is_empty() {
        println!("プラットフォーム: {}", target_platform.cyan());
    }
    // CLIで指定されたregistryを表示（config側のregistryは各サービスビルド時に表示）
    if let Some(reg) = registry {
        println!("レジストリ (CLI): {}", reg.cyan());
    }

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
    let buildable_services: Vec<(&String, &fleetflow_core::Service)> = target_services
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
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // BuildResolver と ImageBuilder を作成
    let resolver = BuildResolver::new(project_root.to_path_buf());
    let builder = ImageBuilder::new(docker_conn.clone());

    // プッシュが必要な場合は ImagePusher も作成
    let pusher = if push {
        Some(ImagePusher::new(docker_conn.clone()))
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
        // registry優先順位: CLI > Service > Stage > Flow
        let effective_registry = registry
            .or(service.registry.as_deref())
            .or(stage_config.registry.as_deref())
            .or(config.registry.as_deref());

        let (base_image, tag) = resolve_tag(
            cli_tag,
            service.image.as_deref().unwrap_or(service_name.as_str()),
        );
        let full_image = if let Some(reg) = effective_registry {
            // registry/{project}-{stage}:{tag} 形式
            format!("{}/{}-{}:{}", reg, config.name, stage_name, tag)
        } else {
            format!("{}:{}", base_image, tag)
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
        println!("  → Image: {}", full_image.cyan());

        // ビルド実行
        if use_buildx && !target_platform.is_empty() {
            // docker buildx build でクロスプラットフォームビルド
            let result = build_with_buildx(
                &dockerfile_path,
                &context_path,
                &full_image,
                target_platform,
                &build_args,
                target.as_deref(),
                no_cache,
                push,
            )
            .await;

            match result {
                Ok(_) => {
                    println!("  {} ビルド完了", "✓".green());
                    build_results.push((service_name.to_string(), full_image));
                }
                Err(e) => {
                    eprintln!("  {} ビルドエラー: {}", "✗".red().bold(), e);
                    return Err(anyhow::anyhow!("ビルドに失敗しました"));
                }
            }
        } else {
            // docker buildxでローカルビルド（BuildKit有効）
            match builder
                .build_image_from_path(
                    &context_path,
                    &dockerfile_path,
                    &full_image,
                    build_args.clone(),
                    target.as_deref(),
                    no_cache,
                    None,
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
    }

    // プッシュ処理（buildxで--push済みの場合はスキップ）
    let already_pushed = use_buildx && push;
    if let Some(pusher) = pusher {
        if already_pushed {
            println!();
            println!("{}", "📤 buildxで既にプッシュ済み".blue().bold());
        } else {
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
