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

    let is_stage_stop = service.is_none();
    if is_stage_stop {
        println!(
            "{}",
            format!(
                "ステージ '{}' の全サービス ({} 個) を停止中...",
                stage_name,
                target_services.len()
            )
            .yellow()
        );
    } else {
        println!(
            "{}",
            format!("サービス '{}' を停止中...", target_services[0]).yellow()
        );
    }

    // Docker接続
    let docker_conn = docker::init_docker_with_error_handling().await?;

    for svc_name in &target_services {
        let container_name = format!("{}-{}-{}", config.name, stage_name, svc_name);

        match docker_conn
            .stop_container(
                &container_name,
                None::<bollard::query_parameters::StopContainerOptions>,
            )
            .await
        {
            Ok(_) => {
                println!("  ✓ '{}' を停止しました", svc_name);
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                println!("  ℹ コンテナ '{}' は存在しません", svc_name);
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 304, ..
            }) => {
                println!("  ℹ '{}' は既に停止しています", svc_name);
            }
            Err(e) => return Err(e.into()),
        }
    }

    println!();
    if is_stage_stop {
        println!(
            "{}",
            format!("✓ ステージ '{}' の全サービスを停止しました", stage_name)
                .green()
                .bold()
        );
    } else {
        println!(
            "{}",
            format!("✓ '{}' を停止しました", target_services[0])
                .green()
                .bold()
        );
    }

    Ok(())
}
