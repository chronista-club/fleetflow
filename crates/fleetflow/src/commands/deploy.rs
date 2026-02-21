use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    project_root: &std::path::Path,
    stage: Option<String>,
    service: Option<String>,
    no_pull: bool,
    no_prune: bool,
    yes: bool,
) -> anyhow::Result<()> {
    println!("{}", "デプロイを開始します...".blue().bold());
    utils::print_loaded_config_files(project_root);

    // ステージ名の決定
    let stage_name = utils::determine_stage_name(stage, config)?;
    println!("ステージ: {}", stage_name.cyan());

    // ステージの取得
    let stage_config = config
        .stages
        .get(&stage_name)
        .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

    // デプロイ対象のサービスを決定（--serviceオプションがあればフィルタ）
    let target_services: Vec<String> = if let Some(ref target) = service {
        // 指定されたサービスがステージに存在するか確認
        if !stage_config.services.contains(target) {
            return Err(anyhow::anyhow!(
                "サービス '{}' はステージ '{}' に存在しません。\n利用可能なサービス: {}",
                target,
                stage_name,
                stage_config.services.join(", ")
            ));
        }
        vec![target.clone()]
    } else {
        stage_config.services.clone()
    };

    println!();
    if service.is_some() {
        println!(
            "{}",
            format!("デプロイ対象サービス (指定: {} 個):", target_services.len()).bold()
        );
    } else {
        println!(
            "{}",
            format!("デプロイ対象サービス ({} 個):", target_services.len()).bold()
        );
    }
    for service_name in &target_services {
        let svc = config.services.get(service_name);
        let image = svc
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
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // 1. 既存コンテナの停止・削除
    println!();
    println!("{}", "【Step 1/5】既存コンテナを停止・削除中...".yellow());
    for service_name in &target_services {
        let container_name = format!("{}-{}-{}", config.name, stage_name, service_name);

        // 停止
        match docker_conn
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
                status_code: 404, ..
            }) => {
                println!("  - {} (コンテナなし)", service_name);
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 304, ..
            }) => {
                println!("  - {} (既に停止中)", service_name);
            }
            Err(e) => {
                println!("  ⚠ {} 停止エラー: {}", service_name, e);
            }
        }

        // 削除（強制）
        match docker_conn
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
                status_code: 404, ..
            }) => {
                // コンテナが存在しない場合は無視
            }
            Err(e) => {
                println!("  ⚠ {} 削除エラー: {}", service_name, e);
            }
        }
    }

    // 2. イメージのpull（デフォルトで実行、--no-pullでスキップ）
    if !no_pull {
        println!();
        println!("{}", "【Step 2/5】最新イメージをダウンロード中...".blue());
        for service_name in &target_services {
            if let Some(svc) = config.services.get(service_name)
                && let Some(image) = &svc.image
            {
                println!("  ↓ {} ({})", service_name.cyan(), image);
                match docker::pull_image(&docker_conn, image).await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("    ⚠ pullエラー: {}", e);
                    }
                }
            }
        }
    } else {
        println!();
        println!("【Step 2/5】イメージpullをスキップ（--no-pull指定）");
    }

    // 3. ネットワーク作成（存在しない場合のみ）
    let network_name = fleetflow_container::get_network_name(&config.name, &stage_name);
    println!();
    println!(
        "{}",
        format!("【Step 3/5】ネットワーク準備中: {}", network_name).blue()
    );

    let network_config = bollard::models::NetworkCreateRequest {
        name: network_name.clone(),
        driver: Some("bridge".to_string()),
        ..Default::default()
    };

    match docker_conn.create_network(network_config).await {
        Ok(_) => {
            println!("  ✓ ネットワーク作成完了");
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 409, ..
        }) => {
            println!("  ✓ ネットワークは既に存在します");
        }
        Err(e) => {
            println!("  ✗ ネットワーク作成エラー: {}", e);
            return Err(e.into());
        }
    }

    // 4. コンテナの作成・起動
    println!();
    println!("{}", "【Step 4/5】コンテナを作成・起動中...".green());

    // 依存関係順にソート（簡易版：depends_onがないものを先に）
    let mut ordered_services: Vec<String> = Vec::new();
    let mut remaining: Vec<String> = target_services.clone();

    // まずdepends_onが空のサービスを追加
    remaining.retain(|name| {
        if let Some(svc) = config.services.get(name)
            && svc.depends_on.is_empty()
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

        let (container_config, create_options) = fleetflow_container::service_to_container_config(
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

        // イメージの存在確認（--no-pullの場合のみ、ローカルになければpull）
        if no_pull {
            match docker_conn.inspect_image(image).await {
                Ok(_) => {}
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => {
                    println!("  ↓ ローカルにイメージがないためpull: {}", image);
                    docker::pull_image(&docker_conn, image).await?;
                }
                Err(e) => return Err(e.into()),
            }
        }

        // コンテナ作成
        match docker_conn
            .create_container(Some(create_options.clone()), container_config.clone())
            .await
        {
            Ok(_) => {
                println!("  ✓ コンテナを作成しました");
            }
            Err(e) => {
                eprintln!("  ✗ コンテナ作成エラー: {}", e);
                return Err(e.into());
            }
        }

        // 依存サービスの待機（wait_forが設定されている場合）
        if let Some(wait_config) = &service_def.wait_for
            && !service_def.depends_on.is_empty()
        {
            println!("  ↻ 依存サービスの準備完了を待機中...");
            for dep_service in &service_def.depends_on {
                let dep_container = format!("{}-{}-{}", config.name, stage_name, dep_service);
                match fleetflow_container::wait_for_service(
                    &docker_conn,
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
        match docker_conn
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
                eprintln!("  ✗ 起動エラー: {}", e);
                return Err(e.into());
            }
        }
    }

    // 5. 不要イメージ・ビルドキャッシュの削除
    if !no_prune {
        println!();
        println!(
            "{}",
            "【Step 5/5】不要イメージ・ビルドキャッシュを削除中...".yellow()
        );

        // 1週間以上古い未使用イメージを削除
        let mut image_filters = std::collections::HashMap::new();
        image_filters.insert("until".to_string(), vec!["168h".to_string()]);
        image_filters.insert("dangling".to_string(), vec!["true".to_string()]);

        let prune_opts = bollard::query_parameters::PruneImagesOptions {
            filters: Some(image_filters),
        };

        match docker_conn.prune_images(Some(prune_opts)).await {
            Ok(result) => {
                let deleted_count = result.images_deleted.as_ref().map(|v| v.len()).unwrap_or(0);
                let reclaimed = result.space_reclaimed.unwrap_or(0);
                if deleted_count > 0 || reclaimed > 0 {
                    let reclaimed_mb = reclaimed as f64 / 1_048_576.0;
                    println!(
                        "  ✓ 不要イメージを削除 ({} 個, {:.1}MB 解放)",
                        deleted_count, reclaimed_mb
                    );
                } else {
                    println!("  ✓ 削除対象のイメージはありません");
                }
            }
            Err(e) => {
                println!("  ⚠ イメージ削除でエラー: {}", e);
            }
        }

        // ビルドキャッシュの削除
        let mut build_filters = std::collections::HashMap::new();
        build_filters.insert("until".to_string(), vec!["168h".to_string()]);

        let build_prune_opts = bollard::query_parameters::PruneBuildOptions {
            filters: Some(build_filters),
            ..Default::default()
        };

        match docker_conn.prune_build(Some(build_prune_opts)).await {
            Ok(result) => {
                let reclaimed = result.space_reclaimed.unwrap_or(0);
                if reclaimed > 0 {
                    let reclaimed_mb = reclaimed as f64 / 1_048_576.0;
                    println!("  ✓ ビルドキャッシュを削除 ({:.1}MB 解放)", reclaimed_mb);
                } else {
                    println!("  ✓ 削除対象のビルドキャッシュはありません");
                }
            }
            Err(e) => {
                println!("  ⚠ ビルドキャッシュ削除でエラー: {}", e);
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

    Ok(())
}
