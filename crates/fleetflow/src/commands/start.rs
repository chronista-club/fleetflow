use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    service: String,
    stage: Option<String>,
) -> anyhow::Result<()> {
    println!("{}", format!("サービス '{}' を起動中...", service).green());

    // ステージ名の決定
    let stage_name = utils::determine_stage_name(stage, config)?;
    println!("ステージ: {}", stage_name.cyan());

    // サービスの存在確認
    let service_def = config
        .services
        .get(&service)
        .ok_or_else(|| anyhow::anyhow!("サービス '{}' が見つかりません", service))?;

    // Docker接続
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // コンテナ名
    let container_name = format!("{}-{}-{}", config.name, stage_name, service);

    // コンテナの起動
    match docker_conn
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

            let (container_config, create_options) =
                fleetflow_container::service_to_container_config(
                    &service,
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

            println!();
            println!(
                "{}",
                format!("✓ '{}' を起動しました", service).green().bold()
            );
        }
        Err(e) => return Err(e.into()),
    }

    Ok(())
}
