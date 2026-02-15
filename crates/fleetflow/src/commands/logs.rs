use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    project_root: &std::path::Path,
    stage: Option<String>,
    service: Option<String>,
    lines: usize,
    follow: bool,
) -> anyhow::Result<()> {
    println!("{}", "ログを取得中...".blue());
    utils::print_loaded_config_files(project_root);

    // Docker接続
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // ステージ名を先に取得
    let stage_name = if let Some(ref _service_name) = service {
        // サービス指定の場合でもステージ名が必要
        stage.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Logsコマンドにはステージ名の指定が必要です（-s/--stage）")
        })?
    } else if let Some(ref s) = stage {
        s
    } else {
        return Err(anyhow::anyhow!(
            "ステージ名を指定してください（-s/--stage）"
        ));
    };

    println!("ステージ: {}", stage_name.cyan());

    // 対象サービスの決定
    let target_services = if let Some(service_name) = service {
        vec![service_name]
    } else {
        let stage_config = config
            .stages
            .get(stage_name)
            .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

        stage_config.services.clone()
    };

    println!();

    // 複数サービスの場合は色を割り当て
    let colors = [
        colored::Color::Cyan,
        colored::Color::Green,
        colored::Color::Yellow,
        colored::Color::Magenta,
        colored::Color::Blue,
    ];

    for (idx, service_name) in target_services.iter().enumerate() {
        // OrbStack連携の命名規則を使用: {project}-{stage}-{service}
        let container_name = format!("{}-{}-{}", config.name, stage_name, service_name);
        let service_color = colors[idx % colors.len()];

        if !follow {
            println!(
                "{}",
                format!("=== {} のログ ===", service_name)
                    .bold()
                    .color(service_color)
            );
        }

        #[allow(deprecated)]
        let options = bollard::container::LogsOptions::<String> {
            follow,
            stdout: true,
            stderr: true,
            tail: lines.to_string(),
            timestamps: true,
            ..Default::default()
        };

        use bollard::container::LogOutput;
        use futures_util::stream::StreamExt;

        let mut log_stream = docker_conn.logs(&container_name, Some(options));

        while let Some(log) = log_stream.next().await {
            match log {
                Ok(output) => {
                    let prefix = format!("[{}]", service_name).color(service_color);

                    match output {
                        LogOutput::StdOut { message } => {
                            let msg = String::from_utf8_lossy(&message);
                            for line in msg.lines() {
                                if !line.is_empty() {
                                    println!("{} {}", prefix, line);
                                }
                            }
                        }
                        LogOutput::StdErr { message } => {
                            let msg = String::from_utf8_lossy(&message);
                            for line in msg.lines() {
                                if !line.is_empty() {
                                    println!("{} {} {}", prefix, "stderr:".red(), line);
                                }
                            }
                        }
                        LogOutput::Console { message } => {
                            let msg = String::from_utf8_lossy(&message);
                            for line in msg.lines() {
                                if !line.is_empty() {
                                    println!("{} {}", prefix, line);
                                }
                            }
                        }
                        LogOutput::StdIn { .. } => {}
                    }
                }
                Err(e) => {
                    eprintln!("  ⚠ ログ取得エラー ({}): {}", service_name, e);
                    break;
                }
            }
        }

        if !follow {
            println!();
        }
    }

    if follow {
        println!();
        println!("{}", "Ctrl+C でログ追跡を終了".dimmed());
    }

    Ok(())
}
