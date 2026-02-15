use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    service: String,
    stage: Option<String>,
) -> anyhow::Result<()> {
    println!("{}", format!("サービス '{}' を停止中...", service).yellow());

    // ステージ名の決定
    let stage_name = utils::determine_stage_name(stage, config)?;
    println!("ステージ: {}", stage_name.cyan());

    // サービスの存在確認
    config
        .services
        .get(&service)
        .ok_or_else(|| anyhow::anyhow!("サービス '{}' が見つかりません", service))?;

    // Docker接続
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // コンテナ名
    let container_name = format!("{}-{}-{}", config.name, stage_name, service);

    // コンテナの停止
    match docker_conn
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

    Ok(())
}
