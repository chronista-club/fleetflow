use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    project_root: &std::path::Path,
    stage: Option<String>,
) -> anyhow::Result<()> {
    utils::print_loaded_config_files(project_root);

    let docker_conn = docker::init_docker_with_error_handling().await?;
    let stage_name = utils::determine_stage_name(stage, config)?;

    let stage_config = config
        .stages
        .get(&stage_name)
        .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

    println!(
        "ステージ: {} ({} サービス)",
        stage_name.cyan(),
        stage_config.services.len()
    );
    println!();

    // ヘッダー
    println!(
        "  {:<16} {:<16} {}",
        "サービス".bold(),
        "状態".bold(),
        "イメージ".bold()
    );
    println!("  {}", "─".repeat(56));

    let mut running_count = 0;

    for svc_name in &stage_config.services {
        let container_name = format!("{}-{}-{}", config.name, stage_name, svc_name);

        let service = config.services.get(svc_name);
        let image = service
            .and_then(|s| s.image.as_ref())
            .map(|i| i.as_str())
            .unwrap_or("(未設定)");

        // コンテナの状態を確認
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
                let status_str = state
                    .and_then(|s| s.status.as_ref())
                    .map(|s| format!("{:?}", s).to_lowercase())
                    .unwrap_or_else(|| "unknown".to_string());

                if running {
                    running_count += 1;
                    format!("✓ {}", status_str).green().to_string()
                } else {
                    format!("■ {}", status_str).yellow().to_string()
                }
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => "✗ missing".red().to_string(),
            Err(_) => "? error".red().to_string(),
        };

        println!("  {:<16} {:<24} {}", svc_name, status, image.dimmed());
    }

    println!();
    let total = stage_config.services.len();
    let summary = format!("概要: {}/{} 稼働中", running_count, total);
    if running_count == total {
        println!("  {}", summary.green().bold());
    } else if running_count == 0 {
        println!("  {}", summary.red().bold());
    } else {
        println!("  {}", summary.yellow().bold());
    }

    Ok(())
}
