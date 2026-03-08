use crate::docker;
use colored::Colorize;
use std::collections::HashMap;

/// サービスのローカルビルドを実行する共通関数
async fn build_service_image(
    docker_conn: &bollard::Docker,
    project_root: &std::path::Path,
    service_name: &str,
    service: &fleetflow_core::Service,
    image: &str,
) -> anyhow::Result<()> {
    println!("  🔨 build設定があるためローカルビルドを実行...");

    let resolver = fleetflow_build::BuildResolver::new(project_root.to_path_buf());

    let dockerfile_path = resolver
        .resolve_dockerfile(service_name, service)?
        .ok_or_else(|| {
            anyhow::anyhow!("Dockerfileが見つかりません: サービス '{}'", service_name)
        })?;

    let context_path = resolver.resolve_context(service)?;

    let variables: HashMap<String, String> = std::env::vars().collect();
    let build_args = resolver.resolve_build_args(service, &variables);
    let target = service.build.as_ref().and_then(|b| b.target.clone());

    println!(
        "  → Dockerfile: {}",
        dockerfile_path.display().to_string().cyan()
    );
    println!("  → Context: {}", context_path.display().to_string().cyan());
    println!("  → Image: {}", image.cyan());

    let builder = fleetflow_build::ImageBuilder::new(docker_conn.clone());
    builder
        .build_image_from_path(
            &context_path,
            &dockerfile_path,
            image,
            build_args,
            target.as_deref(),
            false,
            None,
        )
        .await?;

    println!("  {} ビルド完了", "✓".green());
    Ok(())
}

/// 環境変数のキーがセンシティブかどうか判定する
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("pass")
        || lower.contains("secret")
        || lower.contains("key")
        || lower.contains("token")
}

/// dry-run モードで実行計画を表示する
fn print_dry_run_plan(
    config: &fleetflow_core::Flow,
    stage_name: &str,
    stage_config: &fleetflow_core::Stage,
) -> anyhow::Result<()> {
    println!(
        "{}",
        format!("[dry-run] ステージ '{}' の起動計画:", stage_name)
            .yellow()
            .bold()
    );

    let network_name = fleetflow_container::get_network_name(&config.name, stage_name);
    println!();
    println!("  ネットワーク: {} (作成予定)", network_name.cyan());

    for service_name in &stage_config.services {
        let service = config
            .services
            .get(service_name)
            .ok_or_else(|| anyhow::anyhow!("サービス '{}' の定義が見つかりません", service_name))?;

        let container_name = format!("{}-{}-{}", config.name, stage_name, service_name);
        let image = service.image.as_deref().unwrap_or("(未設定)");

        println!();
        println!("  サービス: {}", service_name.cyan().bold());
        println!("    コンテナ: {}", container_name);
        println!("    イメージ: {}", image);

        // ポートマッピング
        for port in &service.ports {
            let protocol = match port.protocol {
                fleetflow_core::Protocol::Tcp => "tcp",
                fleetflow_core::Protocol::Udp => "udp",
            };
            println!(
                "    ポート: {} \u{2192} {}/{}",
                port.host, port.container, protocol
            );
        }

        // ボリューム
        for vol in &service.volumes {
            let mode = if vol.read_only { "ro" } else { "rw" };
            println!(
                "    ボリューム: {} \u{2192} {} ({})",
                vol.host.display(),
                vol.container.display(),
                mode
            );
        }

        // 環境変数
        if !service.environment.is_empty() {
            let env_strs: Vec<String> = service
                .environment
                .iter()
                .map(|(k, v)| {
                    if is_sensitive_key(k) {
                        format!("{}=***", k)
                    } else {
                        format!("{}={}", k, v)
                    }
                })
                .collect();
            println!("    環境変数: {}", env_strs.join(", "));
        }
    }

    println!();
    println!(
        "{}",
        "[dry-run] 実際の操作は行われません。--dry-run を外して実行してください。"
            .yellow()
            .bold()
    );

    Ok(())
}

pub async fn handle(
    config: &fleetflow_core::Flow,
    project_root: &std::path::Path,
    stage: Option<String>,
    pull: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    // ステージ名の決定（デフォルトステージをサポート）
    let stage_name = crate::utils::determine_stage_name(stage, config)?;

    println!("ステージ: {}", stage_name.cyan());

    // ステージの取得
    let stage_config = config.stages.get(&stage_name).ok_or_else(|| {
        let available: Vec<_> = config.stages.keys().map(|s| s.as_str()).collect();
        anyhow::anyhow!(
            "ステージ '{}' が見つかりません。利用可能: {}",
            stage_name,
            available.join(", ")
        )
    })?;

    // dry-run モードの場合は実行計画を表示して終了
    if dry_run {
        return print_dry_run_plan(config, &stage_name, stage_config);
    }

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
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // ネットワーク作成 (#14)
    let network_name = fleetflow_container::get_network_name(&config.name, &stage_name);
    println!();
    println!("{}", format!("🌐 ネットワーク: {}", network_name).blue());

    docker::ensure_network(&docker_conn, &network_name).await?;

    // 各サービスを起動
    for service_name in &stage_config.services {
        let service = config
            .services
            .get(service_name)
            .ok_or_else(|| anyhow::anyhow!("サービス '{}' の定義が見つかりません", service_name))?;

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
        let (container_config, create_options) = fleetflow_container::service_to_container_config(
            service_name,
            service,
            &stage_name,
            &config.name,
        );

        // build設定がある場合は先にビルドを実行（ローカルビルド優先）
        if service.build.is_some() {
            let image = container_config
                .image
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("イメージ名が指定されていません"))?;

            build_service_image(&docker_conn, project_root, service_name, service, image).await?;
        }

        // --pull フラグが指定されていて、build設定がない場合は最新イメージをpull
        if pull && service.build.is_none() {
            let image = container_config
                .image
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("イメージ名が指定されていません"))?;
            docker::pull_image_always(&docker_conn, image).await?;
        }

        // コンテナ作成
        match docker_conn
            .create_container(Some(create_options.clone()), container_config.clone())
            .await
        {
            Ok(response) => {
                println!("  ✓ コンテナ作成: {}", response.id);

                // コンテナ起動
                docker_conn
                    .start_container(
                        &response.id,
                        None::<bollard::query_parameters::StartContainerOptions>,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("コンテナ起動に失敗: {}", e))?;
                println!("  ✓ 起動完了");
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 409, ..
            }) => {
                // コンテナが既に存在する場合
                println!("  ℹ コンテナは既に存在します");
                let container_name = create_options.name.as_deref().unwrap_or_default();

                // 既存コンテナを起動
                match docker_conn
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
                        docker_conn
                            .restart_container(
                                container_name,
                                None::<bollard::query_parameters::RestartContainerOptions>,
                            )
                            .await
                            .map_err(|e| anyhow::anyhow!("コンテナ再起動に失敗: {}", e))?;
                        println!("  ✓ 再起動完了");
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!("コンテナ起動に失敗: {}", e));
                    }
                }
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                // イメージが見つからない場合
                let image = container_config
                    .image
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("イメージ名が指定されていません"))?;

                if service.build.is_some() {
                    println!("  ℹ イメージが見つかりません: {}", image.cyan());
                    build_service_image(&docker_conn, project_root, service_name, service, image)
                        .await?;
                } else {
                    docker::pull_image(&docker_conn, image).await?;
                }

                // pull/build成功後、再度コンテナ作成を試行
                let response = docker_conn
                    .create_container(Some(create_options.clone()), container_config.clone())
                    .await
                    .map_err(|e| anyhow::anyhow!("コンテナ作成に失敗: {}", e))?;

                println!("  ✓ コンテナ作成: {}", response.id);

                docker_conn
                    .start_container(
                        &response.id,
                        None::<bollard::query_parameters::StartContainerOptions>,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("コンテナ起動に失敗: {}", e))?;
                println!("  ✓ 起動完了");
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
                    eprintln!(
                        "  • 既存のコンテナを停止: fleet down --stage={}",
                        stage_name
                    );
                    eprintln!("  • 別のポート番号を使用してください");
                    eprintln!("  • docker ps でポートを使用しているコンテナを確認してください");
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

    // Readinessチェック: readiness設定があるサービスを確認
    let readiness_services: Vec<_> = stage_config
        .services
        .iter()
        .filter_map(|svc_name| {
            config.services.get(svc_name).and_then(|svc| {
                svc.readiness
                    .as_ref()
                    .map(|r| (svc_name.clone(), r.clone()))
            })
        })
        .collect();

    if !readiness_services.is_empty() {
        println!();
        println!(
            "{}",
            format!(
                "🏥 Readinessチェック ({} サービス)...",
                readiness_services.len()
            )
            .blue()
            .bold()
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;

        for (svc_name, readiness) in &readiness_services {
            let url = format!("http://localhost:{}{}", readiness.port, readiness.path);
            println!("  {} → {}", svc_name.cyan(), url);

            let deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(readiness.timeout);
            let mut ready = false;

            while std::time::Instant::now() < deadline {
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        ready = true;
                        break;
                    }
                    _ => {
                        tokio::time::sleep(std::time::Duration::from_secs(readiness.interval))
                            .await;
                    }
                }
            }

            if ready {
                println!("  {} {} ready", "✓".green(), svc_name.cyan());
            } else {
                println!(
                    "  {} {} は {}秒以内に応答しませんでした",
                    "✗".red(),
                    svc_name.cyan(),
                    readiness.timeout
                );
            }
        }
    }

    println!();
    println!("{}", "✓ すべてのサービスが起動しました！".green().bold());

    Ok(())
}
