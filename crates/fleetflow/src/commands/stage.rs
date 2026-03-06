use crate::docker;
use crate::utils;
use colored::Colorize;

use super::super::StageCommands;

pub async fn handle(
    cmd: StageCommands,
    project_root: &std::path::Path,
    config: &fleetflow_core::Flow,
) -> anyhow::Result<()> {
    match cmd {
        StageCommands::Up { stage, yes, pull } => {
            println!(
                "{}",
                format!("ステージ '{}' を起動中...", stage).blue().bold()
            );
            utils::print_loaded_config_files(project_root);

            // 確認（--yesが指定されていない場合）
            if !yes {
                println!();
                println!(
                    "{}",
                    "警告: 既存のコンテナを停止・削除して再作成します。".yellow()
                );
                println!("実行するには --yes オプションを指定してください");
                std::process::exit(2);
            }

            let stage_config = config.stages.get(&stage).ok_or_else(|| {
                let available: Vec<_> = config.stages.keys().collect();
                anyhow::anyhow!(
                    "ステージ '{}' が見つかりません。利用可能: {:?}",
                    stage,
                    available
                )
            })?;

            // ステージタイプ判定
            let is_remote = !stage_config.servers.is_empty();
            if is_remote {
                println!("  タイプ: {} (インフラ＋コンテナ)", "リモート".cyan());
                println!("  サーバー: {:?}", stage_config.servers);
                println!("  ⚠ リモートステージのインフラ起動は未実装です");
                println!("  現在は 'fleet cloud server up' を使用してください");
            } else {
                println!("  タイプ: {} (コンテナのみ)", "ローカル".green());
            }

            // Docker接続
            println!();
            println!("{}", "Dockerに接続中...".blue());
            let docker_conn = docker::init_docker_with_error_handling().await?;

            // ネットワーク作成
            let network_name = fleetflow_container::get_network_name(&config.name, &stage);
            println!();
            println!("{}", format!("🌐 ネットワーク: {}", network_name).blue());

            let network_config = bollard::models::NetworkCreateRequest {
                name: network_name.clone(),
                driver: Some("bridge".to_string()),
                ..Default::default()
            };
            match docker_conn.create_network(network_config).await {
                Ok(_) => println!("  ✓ ネットワーク作成完了"),
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 409, ..
                }) => {
                    println!("  ✓ ネットワークは既に存在します");
                }
                Err(e) => return Err(e.into()),
            }

            // コンテナ起動
            println!();
            println!(
                "{}",
                format!("サービス起動中 ({} 個):", stage_config.services.len()).bold()
            );

            for service_name in &stage_config.services {
                let service = config.services.get(service_name).ok_or_else(|| {
                    anyhow::anyhow!("サービス '{}' が見つかりません", service_name)
                })?;
                let container_name = format!("{}-{}-{}", config.name, stage, service_name);

                println!();
                println!("  {} {}", "▶".green(), service_name.cyan().bold());

                // イメージ取得
                let image = service.image.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("サービス '{}' にイメージが設定されていません", service_name)
                })?;
                println!("    イメージ: {}", image);

                // pull処理
                if pull {
                    docker::pull_image_always(&docker_conn, image).await?;
                } else {
                    match docker_conn.inspect_image(image).await {
                        Ok(_) => {}
                        Err(bollard::errors::Error::DockerResponseServerError {
                            status_code: 404,
                            ..
                        }) => {
                            docker::pull_image(&docker_conn, image).await?;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }

                // コンテナ設定を作成
                let (container_config, create_options) =
                    fleetflow_container::service_to_container_config(
                        service_name,
                        service,
                        &stage,
                        &config.name,
                    );

                // 既存コンテナの削除
                let _ = docker_conn
                    .remove_container(
                        &container_name,
                        Some(bollard::query_parameters::RemoveContainerOptions {
                            force: true,
                            ..Default::default()
                        }),
                    )
                    .await;

                // コンテナ作成
                docker_conn
                    .create_container(Some(create_options), container_config)
                    .await?;

                // コンテナ起動
                docker_conn
                    .start_container(
                        &container_name,
                        None::<bollard::query_parameters::StartContainerOptions>,
                    )
                    .await?;

                println!("    ✓ 起動完了");
            }

            println!();
            println!(
                "{}",
                format!("✓ ステージ '{}' が起動しました！", stage)
                    .green()
                    .bold()
            );
        }
        StageCommands::Down {
            stage,
            suspend,
            destroy,
            yes,
        } => {
            println!(
                "{}",
                format!("ステージ '{}' を停止中...", stage).yellow().bold()
            );
            utils::print_loaded_config_files(project_root);

            let stage_config = config.stages.get(&stage).ok_or_else(|| {
                let available: Vec<_> = config.stages.keys().collect();
                anyhow::anyhow!(
                    "ステージ '{}' が見つかりません。利用可能: {:?}",
                    stage,
                    available
                )
            })?;

            let is_remote = !stage_config.servers.is_empty();

            // 確認プロンプト（destroyの場合）
            if destroy && !yes {
                println!();
                println!(
                    "{}",
                    "⚠ 警告: --destroy はサーバーを完全に削除します"
                        .red()
                        .bold()
                );
                println!("  データは復旧できません。実行するには --yes を指定してください。");
                return Ok(());
            }

            // Docker接続
            let docker_conn = docker::init_docker_with_error_handling().await?;

            // コンテナ停止
            println!();
            println!(
                "{}",
                format!("サービス停止中 ({} 個):", stage_config.services.len()).bold()
            );

            for service_name in &stage_config.services {
                let container_name = format!("{}-{}-{}", config.name, stage, service_name);
                print!("  {} {} ... ", "■".yellow(), service_name);

                match docker_conn
                    .stop_container(
                        &container_name,
                        None::<bollard::query_parameters::StopContainerOptions>,
                    )
                    .await
                {
                    Ok(_) => println!("{}", "停止".green()),
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 304,
                        ..
                    }) => {
                        println!("{}", "既に停止".dimmed());
                    }
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        ..
                    }) => {
                        println!("{}", "存在しない".dimmed());
                    }
                    Err(e) => println!("{}: {}", "エラー".red(), e),
                }
            }

            // リモートステージの場合のインフラ操作
            if is_remote {
                if suspend {
                    println!();
                    println!("{}", "サーバー電源をOFFにしています...".yellow());
                    println!("  ⚠ サーバー電源OFFは未実装です");
                    println!("  現在は 'fleet cloud server down --suspend' を使用してください");
                } else if destroy {
                    println!();
                    println!("{}", "サーバーを削除しています...".red().bold());
                    println!("  ⚠ サーバー削除は未実装です");
                    println!("  現在は 'fleet cloud server delete' を使用してください");
                }
            }

            println!();
            if destroy {
                println!(
                    "{}",
                    format!("✓ ステージ '{}' を削除しました", stage)
                        .red()
                        .bold()
                );
            } else if suspend {
                println!(
                    "{}",
                    format!("✓ ステージ '{}' を停止・サスペンドしました", stage)
                        .yellow()
                        .bold()
                );
            } else {
                println!(
                    "{}",
                    format!("✓ ステージ '{}' のコンテナを停止しました", stage)
                        .green()
                        .bold()
                );
            }
        }
        StageCommands::Status { stage } => {
            println!("{}", format!("ステージ '{}' の状態:", stage).blue().bold());
            utils::print_loaded_config_files(project_root);

            let stage_config = config.stages.get(&stage).ok_or_else(|| {
                let available: Vec<_> = config.stages.keys().collect();
                anyhow::anyhow!(
                    "ステージ '{}' が見つかりません。利用可能: {:?}",
                    stage,
                    available
                )
            })?;

            let is_remote = !stage_config.servers.is_empty();
            println!();
            println!(
                "タイプ: {}",
                if is_remote {
                    "リモート".cyan()
                } else {
                    "ローカル".green()
                }
            );

            // サーバー情報（リモートの場合）
            if is_remote {
                println!();
                println!("{}", "インフラ:".bold());
                for server_name in &stage_config.servers {
                    if let Some(server) = config.servers.get(server_name) {
                        println!("  {} {}", "•".cyan(), server_name.bold());
                        println!("    プロバイダー: {}", server.provider);
                        println!(
                            "    状態: {}",
                            "(確認には 'fleet cloud server status' を使用)".dimmed()
                        );
                    }
                }
            }

            // コンテナ状態
            println!();
            println!("{}", "サービス:".bold());

            let docker_conn = docker::init_docker_with_error_handling().await?;

            for service_name in &stage_config.services {
                let container_name = format!("{}-{}-{}", config.name, stage, service_name);

                let status = match docker_conn
                    .inspect_container(
                        &container_name,
                        None::<bollard::query_parameters::InspectContainerOptions>,
                    )
                    .await
                {
                    Ok(info) => {
                        let state = info.state.as_ref();
                        let running = state.and_then(|s| s.running).unwrap_or(false);
                        if running {
                            format!("{}", "running".green())
                        } else {
                            format!("{}", "stopped".yellow())
                        }
                    }
                    Err(_) => format!("{}", "not found".dimmed()),
                };

                println!("  {} {} - {}", "•".cyan(), service_name, status);
            }
        }
        StageCommands::Logs {
            stage,
            service,
            follow,
            tail,
        } => {
            let stage_config = config
                .stages
                .get(&stage)
                .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage))?;

            let docker_conn = docker::init_docker_with_error_handling().await?;

            // 対象サービスの決定
            let target_services: Vec<&String> = if let Some(ref svc) = service {
                if !stage_config.services.contains(svc) {
                    return Err(anyhow::anyhow!(
                        "サービス '{}' はステージ '{}' に存在しません",
                        svc,
                        stage
                    ));
                }
                vec![svc]
            } else {
                stage_config.services.iter().collect()
            };

            for service_name in target_services {
                let container_name = format!("{}-{}-{}", config.name, stage, service_name);

                if !follow {
                    println!("{}", format!("=== {} ===", service_name).cyan().bold());
                }

                let options = bollard::query_parameters::LogsOptions {
                    stdout: true,
                    stderr: true,
                    tail: tail.to_string(),
                    follow,
                    ..Default::default()
                };

                use futures_util::StreamExt;
                let mut logs = docker_conn.logs(&container_name, Some(options));

                while let Some(log_result) = logs.next().await {
                    match log_result {
                        Ok(log) => print!("{}", log),
                        Err(e) => {
                            eprintln!("ログ取得エラー: {}", e);
                            break;
                        }
                    }
                }
            }
        }
        StageCommands::Ps { stage } => {
            let docker_conn = docker::init_docker_with_error_handling().await?;

            let stages_to_show: Vec<&String> = if let Some(ref s) = stage {
                if !config.stages.contains_key(s) {
                    return Err(anyhow::anyhow!("ステージ '{}' が見つかりません", s));
                }
                vec![s]
            } else {
                config.stages.keys().collect()
            };

            println!("{}", "STAGE\tSERVICE\t\tSTATUS\t\tPORTS".bold());
            println!("{}", "-----\t-------\t\t------\t\t-----".dimmed());

            for stage_name in stages_to_show {
                let stage_config = config.stages.get(stage_name).unwrap();

                for service_name in &stage_config.services {
                    let container_name = format!("{}-{}-{}", config.name, stage_name, service_name);

                    let (status, ports) = match docker_conn
                        .inspect_container(
                            &container_name,
                            None::<bollard::query_parameters::InspectContainerOptions>,
                        )
                        .await
                    {
                        Ok(info) => {
                            let state = info.state.as_ref();
                            let running = state.and_then(|s| s.running).unwrap_or(false);
                            let status = if running { "running" } else { "stopped" };

                            let ports: String = info
                                .network_settings
                                .as_ref()
                                .and_then(|ns| ns.ports.as_ref())
                                .map(|p| {
                                    p.iter()
                                        .filter_map(|(k, v)| {
                                            v.as_ref().and_then(|bindings| {
                                                bindings.first().map(|b| {
                                                    format!(
                                                        "{}->{}",
                                                        b.host_port.as_deref().unwrap_or("?"),
                                                        k
                                                    )
                                                })
                                            })
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default();

                            (status.to_string(), ports)
                        }
                        Err(_) => ("not found".to_string(), String::new()),
                    };

                    let status_colored = match status.as_str() {
                        "running" => status.green().to_string(),
                        "stopped" => status.yellow().to_string(),
                        _ => status.dimmed().to_string(),
                    };

                    println!(
                        "{}\t{}\t\t{}\t\t{}",
                        stage_name.cyan(),
                        service_name,
                        status_colored,
                        ports
                    );
                }
            }
        }
    }

    Ok(())
}
