use crate::docker;
use crate::utils;
use colored::Colorize;

pub async fn handle(
    config: &fleetflow_core::Flow,
    project_root: &std::path::Path,
    stage: Option<String>,
    services: &[String],
    lines: usize,
    follow: bool,
    since: Option<String>,
) -> anyhow::Result<()> {
    println!("{}", "ログを取得中...".blue());
    utils::print_loaded_config_files(project_root);

    // Docker接続
    let docker_conn = docker::init_docker_with_error_handling().await?;

    // ステージ名の決定（他コマンドと同じロジックを使用）
    let stage_name = utils::determine_stage_name(stage, config)?;

    println!("ステージ: {}", stage_name.cyan());

    // 対象サービスの決定
    let stage_config = config
        .stages
        .get(&stage_name)
        .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

    let target_services =
        utils::filter_services(&stage_config.services, services, &stage_name)?;

    // --since の計算（現在時刻 - duration → Unix timestamp）
    let since_ts = if let Some(ref since_str) = since {
        let duration_secs = utils::parse_duration(since_str)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ts = now.saturating_sub(duration_secs) as i32;
        println!("  ℹ {}前からのログを表示", since_str);
        ts
    } else {
        0
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

        let options = bollard::query_parameters::LogsOptions {
            follow,
            stdout: true,
            stderr: true,
            tail: lines.to_string(),
            timestamps: true,
            since: since_ts,
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
