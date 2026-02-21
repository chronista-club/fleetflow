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
/// (Phase 1 の初期実装ではルーティング情報のみ表示。Docker状態はPhase 2で実装)
pub fn handle_status(registry: &Registry) {
    println!(
        "{}  {}",
        "Fleet Registry:".bold(),
        registry.name.cyan().bold()
    );
    println!();

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
            // Phase 1: SSH接続は行わず、ルーティング情報のみ表示
            println!(
                "  {:<14} {:<10} {:<14} {}",
                route.fleet.green(),
                route.stage,
                route.server.yellow(),
                "未確認".dimmed()
            );
        }
    }
    println!();
    println!(
        "  {}",
        "ヒント: サーバー状態の確認は Phase 2 で実装予定".dimmed()
    );
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
        let remote_cmd = format!(
            "cd {} && fleet deploy -s {} --yes",
            remote_dir, route.stage
        );

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
        println!(
            "  {}",
            "→ 実行するには --yes を付けてください".yellow()
        );
        return Ok(());
    }

    // SSH経由でデプロイ実行
    for route in &target_routes {
        let server = registry.servers.get(&route.server).unwrap();
        let ssh_host = server.ssh_host.as_deref().unwrap();
        let ssh_user = server.ssh_user.as_deref().unwrap_or("root");
        let ssh_target = format!("{}@{}", ssh_user, ssh_host);
        let deploy_path = server.deploy_path.as_deref().unwrap();
        let remote_dir = format!("{}/{}", deploy_path, fleet_name);
        let remote_cmd = format!(
            "cd {} && fleet deploy -s {} --yes",
            remote_dir, route.stage
        );

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
