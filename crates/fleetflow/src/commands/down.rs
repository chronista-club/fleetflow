use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    project_root: &std::path::Path,
    stage: Option<String>,
    remove: bool,
) -> anyhow::Result<()> {
    println!("{}", "ステージを停止中...".yellow());
    utils::print_loaded_config_files(project_root);

    // ステージ名の決定（デフォルトステージをサポート）
    let stage_name = utils::determine_stage_name(stage, config)?;
    println!("ステージ: {}", stage_name.cyan());

    // ステージの取得
    let stage_config = config
        .stages
        .get(&stage_name)
        .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

    // WS2: backend が Quadlet/Compose なら専用経路へ分岐
    match stage_config.backend {
        fleetflow_core::Backend::Docker => {}
        fleetflow_core::Backend::Quadlet => {
            return crate::commands::quadlet::down(config, &stage_name, stage_config, remove).await;
        }
        fleetflow_core::Backend::Compose => {
            return crate::commands::compose::down(
                config,
                project_root,
                &stage_name,
                stage_config,
                remove,
            )
            .await;
        }
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
        match docker_conn
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
                    match docker_conn
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
                status_code: 304, ..
            }) => {
                println!("  ℹ コンテナは既に停止しています");

                // --remove フラグが指定されている場合は削除
                if remove {
                    match docker_conn
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
                status_code: 404, ..
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

        match docker_conn.remove_network(&network_name).await {
            Ok(_) => {
                println!("  ✓ ネットワーク削除完了");
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
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

    Ok(())
}
