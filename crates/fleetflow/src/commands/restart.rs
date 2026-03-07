use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    service: Option<String>,
    stage: Option<String>,
) -> anyhow::Result<()> {
    // ステージ名の決定
    let stage_name = utils::determine_stage_name(stage, config)?;
    println!("ステージ: {}", stage_name.cyan());

    // 対象サービスの決定
    let stage_config = config
        .stages
        .get(&stage_name)
        .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

    let target_services = if let Some(ref svc) = service {
        if !stage_config.services.contains(svc) {
            return Err(anyhow::anyhow!(
                "サービス '{}' はステージ '{}' に含まれていません。\n利用可能なサービス: {}",
                svc,
                stage_name,
                stage_config.services.join(", ")
            ));
        }
        vec![svc.clone()]
    } else {
        stage_config.services.clone()
    };

    let is_stage_restart = service.is_none();
    if is_stage_restart {
        println!(
            "{}",
            format!(
                "ステージ '{}' の全サービス ({} 個) を再起動中...",
                stage_name,
                target_services.len()
            )
            .green()
        );
    } else {
        println!(
            "{}",
            format!("サービス '{}' を再起動中...", target_services[0]).green()
        );
    }

    // Docker接続
    let docker_conn = docker::init_docker_with_error_handling().await?;

    for svc_name in &target_services {
        let service_def = config
            .services
            .get(svc_name)
            .ok_or_else(|| anyhow::anyhow!("サービス '{}' が見つかりません", svc_name))?;

        let container_name = format!("{}-{}-{}", config.name, stage_name, svc_name);

        if is_stage_restart {
            println!();
            println!("{}", format!("▶ {} を再起動中...", svc_name).green().bold());
        }

        // コンテナの停止
        println!("  ↓ コンテナを停止中...");
        match docker_conn
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
                println!("  ℹ コンテナが存在しません");
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 304, ..
            }) => {
                println!("  ℹ コンテナは既に停止しています");
            }
            Err(e) => return Err(e.into()),
        }

        // コンテナの起動
        println!("  ↑ コンテナを起動中...");
        match docker_conn
            .start_container(
                &container_name,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
        {
            Ok(_) => {
                println!("  ✓ コンテナを起動しました");
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                // コンテナが存在しない場合は作成して起動
                println!("  ℹ コンテナが存在しないため、新規作成します");

                let (container_config, create_options) =
                    fleetflow_container::service_to_container_config(
                        svc_name,
                        service_def,
                        &stage_name,
                        &config.name,
                    );

                docker::ensure_container_running(
                    &docker_conn,
                    &container_name,
                    container_config,
                    create_options,
                )
                .await?;
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 304, ..
            }) => {
                println!("  ℹ コンテナは既に起動しています");
            }
            Err(e) => return Err(e.into()),
        }
    }

    println!();
    if is_stage_restart {
        println!(
            "{}",
            format!("✓ ステージ '{}' の全サービスを再起動しました", stage_name)
                .green()
                .bold()
        );
    } else {
        println!(
            "{}",
            format!("✓ '{}' を再起動しました", target_services[0])
                .green()
                .bold()
        );
    }

    Ok(())
}
