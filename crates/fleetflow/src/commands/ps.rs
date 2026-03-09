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

    let options = bollard::query_parameters::ListContainersOptions {
        all,
        filters: Some(filters.unwrap_or_default()),
        ..Default::default()
    };

    let containers = docker_conn.list_containers(Some(options)).await?;

    println!();
    if containers.is_empty() {
        println!("{}", "実行中のコンテナはありません".dimmed());
    } else {
        println!(
            "{}",
            format!(
                "{:<20} {:<15} {:<12} {:<20} {:<50}",
                "NAME", "STATUS", "HEALTH", "IMAGE", "PORTS"
            )
            .bold()
        );
        println!("{}", "─".repeat(117).dimmed());

        for container in &containers {
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

            // Docker status 文字列からヘルス情報を抽出
            let health = if status.contains("(healthy)") {
                "healthy".green()
            } else if status.contains("(unhealthy)") {
                "unhealthy".red()
            } else if status.contains("(health: starting)") {
                "starting".yellow()
            } else {
                "-".dimmed()
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
                "{:<20} {:<15} {:<12} {:<20} {:<50}",
                name.cyan(),
                status_colored,
                health,
                image,
                ports.dimmed()
            );
        }
    }

    Ok(())
}

/// Control Plane 横断クエリ
///
/// - `project` が Some → 特定プロジェクトの全ステージを表示
/// - `project` が None + `stage` が Some → 全プロジェクトの指定ステージを表示
/// - 両方 None → 全プロジェクト・全ステージを表示
pub async fn handle_cp_query(project: Option<&str>, stage: Option<&str>) -> anyhow::Result<()> {
    use super::cp_client;
    use serde_json::json;

    let scope = match (project, stage) {
        (Some(p), Some(s)) => format!("プロジェクト: {} / ステージ: {}", p.cyan(), s.cyan()),
        (Some(p), None) => format!("プロジェクト: {} (全ステージ)", p.cyan()),
        (None, Some(s)) => format!("全プロジェクト / ステージ: {}", s.cyan()),
        (None, None) => "全プロジェクト・全ステージ".to_string(),
    };

    println!("{} {}", "Control Plane 横断クエリ:".bold(), scope);
    println!();

    let (client, _creds) = cp_client::connect().await?;

    // stage チャネルの list_across_projects で横断クエリ
    let payload = match (project, stage) {
        (Some(p), _) => json!({ "project_slug": p }),
        (None, Some(s)) => json!({ "stage_name": s }),
        (None, None) => json!({}),
    };

    let resp = cp_client::request(&client, "stage", "list_across_projects", payload).await?;

    // 結果表示
    println!(
        "{}",
        format!(
            "{:<20} {:<8} {:<15} {:<12} {:<10}",
            "PROJECT", "STAGE", "SERVICE", "STATUS", "HEALTH"
        )
        .bold()
    );
    println!("{}", "─".repeat(65).dimmed());

    if let Some(stages) = resp["stages"].as_array() {
        if stages.is_empty() {
            println!("{}", "該当するステージがありません。".dimmed());
        } else {
            for s in stages {
                let project_name = s["project_name"].as_str().unwrap_or("N/A");
                let stage_name = s["name"].as_str().unwrap_or("N/A");

                // 各ステージのサービス情報があれば表示
                if let Some(services) = s["services"].as_array() {
                    for svc in services {
                        let svc_name = svc["name"].as_str().unwrap_or("N/A");
                        let status = svc["status"].as_str().unwrap_or("unknown");
                        let health = svc["health"].as_str().unwrap_or("-");
                        let status_colored = match status {
                            "running" => status.green(),
                            "stopped" => status.red(),
                            _ => status.yellow(),
                        };
                        let health_colored = match health {
                            "healthy" => health.green(),
                            "unhealthy" => health.red(),
                            _ => health.dimmed(),
                        };
                        println!(
                            "{:<20} {:<8} {:<15} {:<12} {:<10}",
                            project_name.cyan(),
                            stage_name,
                            svc_name,
                            status_colored,
                            health_colored,
                        );
                    }
                } else {
                    println!(
                        "{:<20} {:<8} {:<15} {:<12} {:<10}",
                        project_name.cyan(),
                        stage_name,
                        "-".dimmed(),
                        "-".dimmed(),
                        "-".dimmed(),
                    );
                }
            }
        }
    }

    client.disconnect().await.ok();
    Ok(())
}
