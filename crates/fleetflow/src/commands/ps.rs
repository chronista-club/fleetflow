use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    project_root: &std::path::Path,
    stage: Option<String>,
    all: bool,
) -> anyhow::Result<()> {
    println!("{}", "コンテナ一覧を取得中...".blue());
    utils::print_loaded_config_files(project_root);

    // Docker接続
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // コンテナ一覧を取得
    let filters = if let Some(stage_name) = stage {
        println!("ステージ: {}", stage_name.cyan());

        // ステージに属するサービスのみフィルタ
        let stage_config = config
            .stages
            .get(&stage_name)
            .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

        let mut filter_map = std::collections::HashMap::new();
        // OrbStack連携の命名規則: {project}-{stage}-{service}
        let names: Vec<String> = stage_config
            .services
            .iter()
            .map(|s| format!("{}-{}-{}", config.name, stage_name, s))
            .collect();
        filter_map.insert("name".to_string(), names);
        Some(filter_map)
    } else {
        // fleetflow.project ラベルでフィルタ
        let mut filter_map = std::collections::HashMap::new();
        filter_map.insert(
            "label".to_string(),
            vec![format!("fleetflow.project={}", config.name)],
        );
        Some(filter_map)
    };

    #[allow(deprecated)]
    let options = bollard::container::ListContainersOptions {
        all,
        filters: filters.unwrap_or_default(),
        ..Default::default()
    };

    #[allow(deprecated)]
    let containers = docker_conn.list_containers(Some(options)).await?;

    println!();
    if containers.is_empty() {
        println!("{}", "実行中のコンテナはありません".dimmed());
    } else {
        println!(
            "{}",
            format!(
                "{:<20} {:<15} {:<20} {:<50}",
                "NAME", "STATUS", "IMAGE", "PORTS"
            )
            .bold()
        );
        println!("{}", "─".repeat(105).dimmed());

        for container in containers {
            let name = container
                .names
                .as_ref()
                .and_then(|n| n.first())
                .map(|n| n.trim_start_matches('/'))
                .unwrap_or("N/A");

            let status = container.status.as_deref().unwrap_or("N/A");
            let status_colored = if status.contains("Up") {
                status.green()
            } else {
                status.red()
            };

            let image = container.image.as_deref().unwrap_or("N/A");

            let ports = container
                .ports
                .as_ref()
                .map(|ports| {
                    ports
                        .iter()
                        .filter_map(|p| {
                            p.public_port
                                .map(|pub_port| format!("{}:{}", pub_port, p.private_port))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();

            println!(
                "{:<20} {:<15} {:<20} {:<50}",
                name.cyan(),
                status_colored,
                image,
                ports.dimmed()
            );
        }
    }

    Ok(())
}
