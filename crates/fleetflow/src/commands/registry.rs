//! fleet registry コマンドハンドラ

use colored::Colorize;
use fleetflow_registry::{Registry, find_registry, parse_registry_file, registry_root};
use std::process::Command;

/// fleet registry list — 全fleetとサーバーの一覧
pub fn handle_list(registry: &Registry) {
    println!(
        "{}  {}",
        "Fleet Registry:".bold(),
        registry.name.cyan().bold()
    );
    println!();

    // Fleets
    println!("{}", "Fleets:".bold());
    if registry.fleets.is_empty() {
        println!("  {}", "(なし)".dimmed());
    } else {
        for (name, entry) in &registry.fleets {
            let desc = entry
                .description
                .as_deref()
                .unwrap_or("")
                .dimmed()
                .to_string();
            println!(
                "  {:<14} {:<24} {}",
                name.green(),
                entry.path.display().to_string().dimmed(),
                desc
            );
        }
    }
    println!();

    // Servers
    println!("{}", "Servers:".bold());
    if registry.servers.is_empty() {
        println!("  {}", "(なし)".dimmed());
    } else {
        for (name, server) in &registry.servers {
            let plan = server.plan.as_deref().unwrap_or("-");
            println!(
                "  {:<14} {:<24} {}",
                name.yellow(),
                server.provider.dimmed(),
                plan.dimmed()
            );
        }
    }
    println!();

    // Routes
    println!("{}", "Routes:".bold());
    if registry.routes.is_empty() {
        println!("  {}", "(なし)".dimmed());
    } else {
        for route in &registry.routes {
            println!(
                "  {}:{:<10} {} {}",
                route.fleet.green(),
                route.stage,
                "→".dimmed(),
                route.server.yellow()
            );
        }
    }
}

/// fleet registry status — 各fleet × serverの稼働状態
/// CP 接続可能な場合は横断クエリで実稼働状態を取得、不可の場合はルーティング情報のみ表示
pub fn handle_status(registry: &Registry) {
    println!(
        "{}  {}",
        "Fleet Registry:".bold(),
        registry.name.cyan().bold()
    );
    println!();

    // CP 接続を試みて実稼働状態を取得
    let rt = tokio::runtime::Handle::try_current();
    let cp_status = if rt.is_ok() {
        // 既に tokio ランタイム内 — ブロック不可、スキップ
        None
    } else {
        // CP に接続してステージ状態を取得（ベストエフォート）
        try_fetch_cp_status()
    };

    println!(
        "  {:<14} {:<10} {:<14} {}",
        "Fleet".bold(),
        "Stage".bold(),
        "Server".bold(),
        "Status".bold()
    );
    println!("  {}", "─".repeat(52).dimmed());

    if registry.routes.is_empty() {
        println!("  {}", "(ルーティング未定義)".dimmed());
    } else {
        for route in &registry.routes {
            let status = cp_status
                .as_ref()
                .and_then(|stages| {
                    stages.iter().find(|s| {
                        s["project_name"].as_str() == Some(route.fleet.as_str())
                            && s["name"].as_str() == Some(route.stage.as_str())
                    })
                })
                .map(|s| {
                    let svc_count = s["services"]
                        .as_array()
                        .map(|arr| arr.len())
                        .unwrap_or(0);
                    let running = s["services"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter(|svc| svc["status"].as_str() == Some("running"))
                                .count()
                        })
                        .unwrap_or(0);
                    if running == svc_count && svc_count > 0 {
                        format!("{}/{} running", running, svc_count).green().to_string()
                    } else if running > 0 {
                        format!("{}/{} running", running, svc_count).yellow().to_string()
                    } else {
                        "stopped".red().to_string()
                    }
                })
                .unwrap_or_else(|| "未確認".dimmed().to_string());

            println!(
                "  {:<14} {:<10} {:<14} {}",
                route.fleet.green(),
                route.stage,
                route.server.yellow(),
                status
            );
        }
    }

    if cp_status.is_none() {
        println!();
        println!(
            "  {}",
            "ヒント: `fleet login` で CP に接続すると実稼働状態を表示します".dimmed()
        );
    }
}

/// CP からステージ横断情報を取得（ベストエフォート）
fn try_fetch_cp_status() -> Option<Vec<serde_json::Value>> {
    use super::cp_client;
    use serde_json::json;

    let rt = tokio::runtime::Runtime::new().ok()?;

    rt.block_on(async {
        let (client, _creds) = cp_client::connect().await.ok()?;
        let resp = cp_client::request(&client, "stage", "list_across_projects", json!({}))
            .await
            .ok()?;
        client.disconnect().await.ok();
        resp["stages"].as_array().cloned()
    })
}

/// fleet registry sync — Registry定義をControl Planeに同期
///
/// Registry 内の全 fleet を CP の project として登録し、
/// 全 server を CP の server として登録する。
pub async fn handle_sync(registry: &Registry) -> anyhow::Result<()> {
    use super::cp_client;
    use serde_json::json;

    println!(
        "{}  {}",
        "Registry Sync:".bold(),
        registry.name.cyan().bold()
    );
    println!();

    let (client, _creds) = cp_client::connect().await?;

    // Fleet → Project として登録
    println!("{}", "プロジェクト同期:".bold());
    for (name, entry) in &registry.fleets {
        let desc = entry.description.as_deref().unwrap_or("");
        let resp = cp_client::request(
            &client,
            "project",
            "create",
            json!({
                "tenant_slug": "default",
                "name": name,
                "slug": name,
                "description": desc,
            }),
        )
        .await;

        match resp {
            Ok(r) if r.get("project").is_some() => {
                println!("  {} {}", "✓".green(), name.cyan());
            }
            Ok(_) => {
                // 既に存在する可能性
                println!("  {} {} (既に存在)", "○".dimmed(), name.cyan());
            }
            Err(e) => {
                println!("  {} {} — {}", "✗".red(), name, e);
            }
        }
    }
    println!();

    // Server として登録
    println!("{}", "サーバー同期:".bold());
    for (name, server) in &registry.servers {
        let ip = server.ssh_host.as_deref().unwrap_or("");
        let resp = cp_client::request(
            &client,
            "server",
            "register",
            json!({
                "tenant_slug": "default",
                "slug": name,
                "hostname": name,
                "provider": server.provider,
                "ip_address": ip,
            }),
        )
        .await;

        match resp {
            Ok(r) if r.get("server").is_some() => {
                println!("  {} {}", "✓".green(), name.yellow());
            }
            Ok(_) => {
                println!("  {} {} (既に存在)", "○".dimmed(), name.yellow());
            }
            Err(e) => {
                println!("  {} {} — {}", "✗".red(), name, e);
            }
        }
    }

    client.disconnect().await.ok();

    println!();
    println!("{}", "同期完了".green().bold());
    Ok(())
}

/// fleet registry deploy <fleet> — Registry定義に従ってSSH経由でデプロイ
pub async fn handle_deploy(
    registry: &Registry,
    registry_root_path: &std::path::Path,
    fleet_name: &str,
    stage: Option<&str>,
    yes: bool,
) -> anyhow::Result<()> {
    // 対象fleetの情報を取得
    let fleet_entry = registry
        .fleets
        .get(fleet_name)
        .ok_or_else(|| anyhow::anyhow!("Fleet '{}' が見つかりません", fleet_name))?;

    // ルートを解決
    let routes = registry.routes_for_fleet(fleet_name);
    if routes.is_empty() {
        anyhow::bail!(
            "Fleet '{}' のデプロイルートが定義されていません",
            fleet_name
        );
    }

    // ステージが指定されている場合はフィルタ
    let target_routes: Vec<_> = if let Some(stage_name) = stage {
        routes
            .into_iter()
            .filter(|r| r.stage == stage_name)
            .collect()
    } else {
        routes
    };

    if target_routes.is_empty()
        && let Some(stage_name) = stage
    {
        anyhow::bail!(
            "Fleet '{}' stage '{}' のデプロイルートが見つかりません",
            fleet_name,
            stage_name
        );
    }

    // fleet プロジェクトのパスを解決
    let fleet_path = registry_root_path.join(&fleet_entry.path);
    if !fleet_path.exists() {
        anyhow::bail!(
            "Fleet ディレクトリが見つかりません: {}",
            fleet_path.display()
        );
    }

    println!(
        "{}  {} ({})",
        "Deploy:".bold(),
        fleet_name.green().bold(),
        fleet_path.display().to_string().dimmed()
    );
    println!();

    // デプロイ計画を表示
    for route in &target_routes {
        let server = registry
            .servers
            .get(&route.server)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' が見つかりません", route.server))?;

        let ssh_host = server.ssh_host.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Server '{}' に ssh-host が設定されていません。fleet-registry.kdl に ssh-host を追加してください",
                route.server
            )
        })?;
        let ssh_user = server.ssh_user.as_deref().unwrap_or("root");
        let ssh_target = format!("{}@{}", ssh_user, ssh_host);

        let deploy_path = server.deploy_path.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Server '{}' に deploy-path が設定されていません",
                route.server
            )
        })?;

        let remote_dir = format!("{}/{}", deploy_path, fleet_name);
        let remote_cmd = format!("cd {} && fleet deploy -s {} --yes", remote_dir, route.stage);

        println!(
            "  {} {}:{} → {} ({})",
            "Route:".bold(),
            route.fleet.green(),
            route.stage,
            route.server.yellow(),
            server.provider.dimmed()
        );
        println!("  {} {}", "SSH:".dimmed(), ssh_target);
        println!("  {} {}", "CMD:".dimmed(), remote_cmd);
        println!();
    }

    // --yes がなければ計画表示のみで終了
    if !yes {
        println!("  {}", "→ 実行するには --yes を付けてください".yellow());
        return Ok(());
    }

    // SSH経由でデプロイ実行
    for route in &target_routes {
        let server = registry
            .servers
            .get(&route.server)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' が見つかりません", route.server))?;
        let ssh_host = server.ssh_host.as_deref().ok_or_else(|| {
            anyhow::anyhow!("Server '{}' に ssh-host が設定されていません", route.server)
        })?;
        let ssh_user = server.ssh_user.as_deref().unwrap_or("root");
        let ssh_target = format!("{}@{}", ssh_user, ssh_host);
        let deploy_path = server.deploy_path.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Server '{}' に deploy-path が設定されていません",
                route.server
            )
        })?;
        let remote_dir = format!("{}/{}", deploy_path, fleet_name);
        let remote_cmd = format!("cd {} && fleet deploy -s {} --yes", remote_dir, route.stage);

        println!(
            "{}",
            format!(
                "▶ {}:{} → {} にデプロイ中...",
                route.fleet, route.stage, route.server
            )
            .cyan()
            .bold()
        );
        println!("  $ ssh {} \"{}\"", ssh_target, remote_cmd);
        println!();

        let status = Command::new("ssh")
            .arg(&ssh_target)
            .arg(&remote_cmd)
            .status()?;

        if !status.success() {
            anyhow::bail!(
                "デプロイ失敗: {}:{} → {} (exit code: {})",
                route.fleet,
                route.stage,
                route.server,
                status.code().unwrap_or(-1)
            );
        }

        println!();
        println!(
            "  {} {}:{} → {} デプロイ完了",
            "✓".green().bold(),
            route.fleet.green(),
            route.stage,
            route.server.yellow()
        );
        println!();
    }

    println!("{}", "全ルートのデプロイが完了しました".green().bold());
    Ok(())
}

/// Registry をロードするヘルパー
pub fn load_registry() -> anyhow::Result<(Registry, std::path::PathBuf)> {
    let registry_path =
        find_registry().ok_or_else(|| anyhow::anyhow!("fleet-registry.kdl が見つかりません"))?;

    let root = registry_root(&registry_path)
        .ok_or_else(|| anyhow::anyhow!("Registry ルートの解決に失敗"))?
        .to_path_buf();

    let registry = parse_registry_file(&registry_path)
        .map_err(|e| anyhow::anyhow!("Registry パースエラー: {}", e))?;

    Ok((registry, root))
}
