use crate::docker;
use crate::self_update;
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

pub async fn handle(
    config: &fleetflow_core::Flow,
    project_root: &std::path::Path,
    stage: Option<String>,
    pull: bool,
) -> anyhow::Result<()> {
    // 最初にバージョンチェック
    self_update::check_and_update_if_needed().await?;

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
            #[allow(deprecated)]
            let image = container_config
                .image
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("イメージ名が指定されていません"))?;

            build_service_image(&docker_conn, project_root, service_name, service, image).await?;
        }

        // --pull フラグが指定されていて、build設定がない場合は最新イメージをpull
        if pull && service.build.is_none() {
            #[allow(deprecated)]
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
                #[allow(deprecated)]
                let container_name = &create_options.name;

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
                #[allow(deprecated)]
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

    println!();
    println!("{}", "✓ すべてのサービスが起動しました！".green().bold());

    Ok(())
}
