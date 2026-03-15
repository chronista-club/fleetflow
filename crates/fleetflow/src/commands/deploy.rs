use crate::docker;
use crate::utils;
use colored::Colorize;
use fleetflow_container::{DeployEngine, DeployEvent, DeployRequest};

/// 環境変数のキーがセンシティブかどうか判定する
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("pass")
        || lower.contains("secret")
        || lower.contains("key")
        || lower.contains("token")
}

/// dry-run モードでデプロイ計画を表示する
fn print_dry_run_plan(
    config: &fleetflow_core::Flow,
    stage_name: &str,
    target_services: &[String],
) -> anyhow::Result<()> {
    println!(
        "{}",
        format!("[dry-run] ステージ '{}' のデプロイ計画:", stage_name)
            .yellow()
            .bold()
    );

    let network_name = fleetflow_container::get_network_name(&config.name, stage_name);
    println!();
    println!("  ネットワーク: {} (作成予定)", network_name.cyan());

    for service_name in target_services {
        let service = config
            .services
            .get(service_name)
            .ok_or_else(|| anyhow::anyhow!("サービス '{}' の定義が見つかりません", service_name))?;

        let container_name = format!("{}-{}-{}", config.name, stage_name, service_name);
        let image = service.image.as_deref().unwrap_or("(未設定)");

        println!();
        println!("  サービス: {}", service_name.cyan().bold());
        println!("    コンテナ: {} (停止・削除→再作成)", container_name);
        println!("    イメージ: {}", image);

        // ポートマッピング
        for port in &service.ports {
            let protocol = match port.protocol {
                fleetflow_core::Protocol::Tcp => "tcp",
                fleetflow_core::Protocol::Udp => "udp",
            };
            println!(
                "    ポート: {} \u{2192} {}/{}",
                port.host, port.container, protocol
            );
        }

        // ボリューム
        for vol in &service.volumes {
            let mode = if vol.read_only { "ro" } else { "rw" };
            println!(
                "    ボリューム: {} \u{2192} {} ({})",
                vol.host.display(),
                vol.container.display(),
                mode
            );
        }

        // 環境変数
        if !service.environment.is_empty() {
            let env_strs: Vec<String> = service
                .environment
                .iter()
                .map(|(k, v)| {
                    if is_sensitive_key(k) {
                        format!("{}=***", k)
                    } else {
                        format!("{}={}", k, v)
                    }
                })
                .collect();
            println!("    環境変数: {}", env_strs.join(", "));
        }
    }

    println!();
    println!(
        "{}",
        "[dry-run] 実際の操作は行われません。--dry-run を外して実行してください。"
            .yellow()
            .bold()
    );

    Ok(())
}

/// DeployEvent を CLI 表示に変換する
fn print_deploy_event(event: DeployEvent) {
    match event {
        DeployEvent::StepStarted {
            step,
            total,
            description,
        } => {
            println!();
            let msg = format!("【Step {}/{}】{}", step, total, description);
            match step {
                1 | 5 => println!("{}", msg.yellow()),
                2 | 3 => println!("{}", msg.blue()),
                4 => println!("{}", msg.green()),
                _ => println!("{}", msg),
            }
        }
        DeployEvent::ServiceProgress { service, action } => match action.as_str() {
            "stopped" => println!("  ✓ {} を停止しました", service.cyan()),
            "removed" => println!("  ✓ {} を削除しました", service.cyan()),
            "creating" => {
                println!();
                println!("{}", format!("■ {} を起動中...", service).green().bold());
            }
            "started" => println!("  ✓ 起動完了"),
            action if action.starts_with("pulling") => {
                println!("  ↓ {} ({})", service.cyan(), &action[8..]);
            }
            _ => println!("  {} {}", service, action),
        },
        DeployEvent::StepCompleted { .. } => {}
        DeployEvent::Completed {
            services_deployed: _,
        } => {}
        DeployEvent::Error { message } => {
            eprintln!("  ✗ {}", message.red());
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn handle(
    config: &fleetflow_core::Flow,
    project_root: &std::path::Path,
    stage: Option<String>,
    services: &[String],
    no_pull: bool,
    no_prune: bool,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    println!("{}", "デプロイを開始します...".blue().bold());
    utils::print_loaded_config_files(project_root);

    // ステージ名の決定
    let stage_name = utils::determine_stage_name(stage, config)?;
    println!("ステージ: {}", stage_name.cyan());

    // ステージの取得
    let stage_config = config
        .stages
        .get(&stage_name)
        .ok_or_else(|| anyhow::anyhow!("ステージ '{}' が見つかりません", stage_name))?;

    // デプロイ対象のサービスを決定（--serviceオプションがあればフィルタ）
    let target_services = utils::filter_services(&stage_config.services, services, &stage_name)?;

    println!();
    if !services.is_empty() {
        println!(
            "{}",
            format!("デプロイ対象サービス (指定: {} 個):", target_services.len()).bold()
        );
    } else {
        println!(
            "{}",
            format!("デプロイ対象サービス ({} 個):", target_services.len()).bold()
        );
    }
    for service_name in &target_services {
        let svc = config.services.get(service_name);
        let image = svc
            .and_then(|s| s.image.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("(イメージ未設定)");
        println!("  • {} ({})", service_name.cyan(), image);
    }

    // dry-run モードの場合は実行計画を表示して終了
    if dry_run {
        return print_dry_run_plan(config, &stage_name, &target_services);
    }

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

    // リモートステージ判定
    let is_remote = !stage_config.servers.is_empty();

    if is_remote {
        // リモート: CP 経由で DeployEngine を実行
        deploy_remote(config, &stage_name, &target_services, no_pull, no_prune).await?;
    } else {
        // ローカル: DeployEngine を直接実行
        deploy_local(config, &stage_name, &target_services, no_pull, no_prune).await?;
    }

    println!();
    println!(
        "{}",
        format!("✓ デプロイ完了: ステージ '{}'", stage_name)
            .green()
            .bold()
    );

    Ok(())
}

/// ローカルデプロイ — DeployEngine を直接実行
async fn deploy_local(
    config: &fleetflow_core::Flow,
    stage_name: &str,
    target_services: &[String],
    no_pull: bool,
    no_prune: bool,
) -> anyhow::Result<()> {
    println!();
    println!("{}", "Dockerに接続中...".blue());
    let docker_conn = docker::init_docker_with_error_handling().await?;

    let engine = DeployEngine::new(docker_conn);
    let request = DeployRequest {
        flow: config.clone(),
        stage_name: stage_name.to_string(),
        target_services: target_services.to_vec(),
        no_pull,
        no_prune,
    };

    engine.execute(&request, print_deploy_event).await?;

    Ok(())
}

/// リモートデプロイ — CP 経由で DeployEngine を実行
async fn deploy_remote(
    config: &fleetflow_core::Flow,
    stage_name: &str,
    target_services: &[String],
    no_pull: bool,
    no_prune: bool,
) -> anyhow::Result<()> {
    use super::cp_client;
    use serde_json::json;

    println!();
    println!("{}", "Control Plane に接続中...".blue());
    let (client, creds) = cp_client::connect().await?;

    let request = DeployRequest {
        flow: config.clone(),
        stage_name: stage_name.to_string(),
        target_services: target_services.to_vec(),
        no_pull,
        no_prune,
    };

    let resp = cp_client::request(
        &client,
        "deploy",
        "execute",
        json!({
            "tenant_slug": creds.tenant_slug.as_deref().unwrap_or("default"),
            "project_slug": config.name,
            "request": serde_json::to_value(&request)?,
        }),
    )
    .await?;

    client.disconnect().await.ok();

    // 結果表示
    let status = resp["status"].as_str().unwrap_or("unknown");
    if status == "success" {
        println!("  ✓ リモートデプロイ成功");
    } else {
        let log = resp["log"].as_str().unwrap_or("");
        eprintln!("  ✗ リモートデプロイ失敗: {}", log);
        anyhow::bail!("リモートデプロイが失敗しました");
    }

    if let Some(log) = resp["log"].as_str()
        && !log.is_empty()
    {
        println!();
        println!("{}", "デプロイログ:".bold());
        println!("{}", log);
    }

    Ok(())
}
