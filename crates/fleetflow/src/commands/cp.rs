//! Control Plane リソース管理コマンド
//!
//! fleet tenant / fleet project / fleet server の各サブコマンドを処理する。
//! Unison Protocol 経由で CP サーバーに接続してリクエストを送信する。

use anyhow::Result;
use colored::Colorize;
use serde_json::json;

use super::cp_client;
use crate::{
    BuildCommands, CostCommands, DnsCommands, ProjectCommands, RemoteCommands, ServerCommands,
    StageCommands, TenantCommands, VolumeCommands,
};

pub async fn handle_tenant(cmd: &TenantCommands) -> Result<()> {
    match cmd {
        TenantCommands::Status => {
            let (client, creds) = cp_client::connect().await?;

            println!("{}", "テナント状態".bold());
            println!();

            let slug = creds.tenant_slug.as_deref().unwrap_or("default");
            let resp =
                cp_client::request(&client, "tenant", "get", json!({ "slug": slug })).await?;

            if let Some(tenant) = resp.get("tenant") {
                println!(
                    "  Name: {}",
                    tenant["name"].as_str().unwrap_or("N/A").cyan()
                );
                println!(
                    "  Slug: {}",
                    tenant["slug"].as_str().unwrap_or("N/A").cyan()
                );
            } else {
                println!("{}", "テナント情報が見つかりません。".yellow());
            }

            client.disconnect().await.ok();
        }
        TenantCommands::List => {
            let (client, _creds) = cp_client::connect().await?;

            println!("{}", "テナント一覧".bold());
            println!();

            let resp = cp_client::request(&client, "tenant", "list", json!({})).await?;

            if let Some(tenants) = resp["tenants"].as_array() {
                if tenants.is_empty() {
                    println!("{}", "テナントがありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!("{:<20} {:<25} {:<15}", "SLUG", "NAME", "PLAN").bold()
                    );
                    println!("{}", "─".repeat(60).dimmed());
                    for t in tenants {
                        println!(
                            "{:<20} {:<25} {:<15}",
                            t["slug"].as_str().unwrap_or("N/A").cyan(),
                            t["name"].as_str().unwrap_or("N/A"),
                            t["plan"].as_str().unwrap_or("N/A").dimmed(),
                        );
                    }
                }
            }

            client.disconnect().await.ok();
        }
        TenantCommands::Create { slug, name, plan } => {
            let (client, _creds) = cp_client::connect().await?;

            println!("{}", "テナント作成".bold());

            let resp = cp_client::request(
                &client,
                "tenant",
                "create",
                json!({
                    "slug": slug,
                    "name": name,
                    "plan": plan,
                }),
            )
            .await?;

            if resp.get("tenant").is_some() {
                println!("{} {}", "作成完了:".green(), slug.cyan());
            }

            client.disconnect().await.ok();
        }
    }

    Ok(())
}

pub async fn handle_project(cmd: &ProjectCommands) -> Result<()> {
    let (client, _creds) = cp_client::connect().await?;

    match cmd {
        ProjectCommands::List => {
            println!("{}", "プロジェクト一覧".bold());
            println!();

            let resp = cp_client::request(&client, "project", "list", json!({})).await?;

            if let Some(projects) = resp["projects"].as_array() {
                if projects.is_empty() {
                    println!("{}", "プロジェクトがありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!("{:<25} {:<30} {:<20}", "SLUG", "NAME", "CREATED").bold()
                    );
                    println!("{}", "─".repeat(75).dimmed());
                    for p in projects {
                        println!(
                            "{:<25} {:<30} {:<20}",
                            p["slug"].as_str().unwrap_or("N/A").cyan(),
                            p["name"].as_str().unwrap_or("N/A"),
                            p["created_at"].as_str().unwrap_or("N/A").dimmed(),
                        );
                    }
                }
            }
        }
        ProjectCommands::Create { slug, name } => {
            println!("{}", "プロジェクト作成".bold());

            let resp = cp_client::request(
                &client,
                "project",
                "create",
                json!({
                    "tenant_slug": "default",
                    "name": name,
                    "slug": slug,
                }),
            )
            .await?;

            if resp.get("project").is_some() {
                println!("{} {}", "作成完了:".green(), slug.cyan());
            }
        }
        ProjectCommands::Show { slug } => {
            println!("{} {}", "プロジェクト詳細:".bold(), slug.cyan());
            println!();

            let resp =
                cp_client::request(&client, "project", "get", json!({ "slug": slug })).await?;

            if let Some(project) = resp.get("project") {
                println!(
                    "  Name:        {}",
                    project["name"].as_str().unwrap_or("N/A").cyan()
                );
                println!(
                    "  Slug:        {}",
                    project["slug"].as_str().unwrap_or("N/A").cyan()
                );
                println!(
                    "  Description: {}",
                    project["description"].as_str().unwrap_or("N/A")
                );
                println!(
                    "  Created:     {}",
                    project["created_at"].as_str().unwrap_or("N/A").dimmed()
                );
            } else {
                println!("{}", "プロジェクトが見つかりません。".yellow());
            }
        }
    }

    client.disconnect().await.ok();
    Ok(())
}

pub async fn handle_server(cmd: &ServerCommands) -> Result<()> {
    let (client, _creds) = cp_client::connect().await?;

    match cmd {
        ServerCommands::List => {
            println!("{}", "サーバー一覧".bold());
            println!();

            let resp = cp_client::request(&client, "server", "list", json!({})).await?;

            if let Some(servers) = resp["servers"].as_array() {
                if servers.is_empty() {
                    println!("{}", "サーバーがありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!(
                            "{:<15} {:<20} {:<15} {:<10}",
                            "SLUG", "PROVIDER", "SSH HOST", "STATUS"
                        )
                        .bold()
                    );
                    println!("{}", "─".repeat(60).dimmed());
                    for s in servers {
                        let status = s["status"].as_str().unwrap_or("unknown");
                        let status_colored = match status {
                            "online" => status.green(),
                            "offline" => status.red(),
                            _ => status.yellow(),
                        };
                        println!(
                            "{:<15} {:<15} {:<20} {:<10}",
                            s["slug"].as_str().unwrap_or("N/A").cyan(),
                            s["provider"].as_str().unwrap_or("N/A"),
                            s["ssh_host"].as_str().unwrap_or("N/A"),
                            status_colored,
                        );
                    }
                }
            }
        }
        ServerCommands::Register {
            slug,
            provider,
            ssh_host,
            deploy_path,
        } => {
            println!("{}", "サーバー登録".bold());

            let mut payload = json!({
                "tenant_slug": "default",
                "slug": slug,
                "hostname": slug,
                "provider": provider,
            });

            if let Some(host) = ssh_host {
                payload["ssh_host"] = json!(host);
            }
            if let Some(path) = deploy_path {
                payload["deploy_path"] = json!(path);
            }

            let resp = cp_client::request(&client, "server", "register", payload).await?;

            if resp.get("server").is_some() {
                println!("{} {}", "登録完了:".green(), slug.cyan());
            }
        }
        ServerCommands::Status { slug } => {
            println!("{} {}", "サーバー状態:".bold(), slug.cyan());
            println!();

            // server チャネルには get メソッドがないので list から検索
            let resp = cp_client::request(&client, "server", "list", json!({})).await?;

            if let Some(servers) = resp["servers"].as_array() {
                if let Some(server) = servers
                    .iter()
                    .find(|s| s["slug"].as_str() == Some(slug.as_str()))
                {
                    println!(
                        "  Slug:      {}",
                        server["slug"].as_str().unwrap_or("N/A").cyan()
                    );
                    println!(
                        "  Provider:  {}",
                        server["provider"].as_str().unwrap_or("N/A")
                    );
                    println!(
                        "  SSH Host:  {}",
                        server["ssh_host"].as_str().unwrap_or("N/A")
                    );
                    let status = server["status"].as_str().unwrap_or("unknown");
                    let status_colored = match status {
                        "online" => status.green(),
                        "offline" => status.red(),
                        _ => status.yellow(),
                    };
                    println!("  Status:    {}", status_colored);
                } else {
                    println!("{}", "サーバーが見つかりません。".yellow());
                }
            }
        }
        ServerCommands::Check => {
            println!("{}", "全サーバー ヘルスチェック（Tailscale）".bold());
            println!();

            let resp = cp_client::request(&client, "server", "check-all", json!({})).await?;

            if let Some(results) = resp["results"].as_array() {
                let updated = resp["updated"].as_u64().unwrap_or(0);
                println!("{}", format!("{:<20} {:<10}", "SERVER", "STATUS").bold());
                println!("{}", "─".repeat(30).dimmed());
                for r in results {
                    let status = r["status"].as_str().unwrap_or("unknown");
                    let status_colored = match status {
                        "online" => status.green(),
                        "offline" => status.red(),
                        _ => status.yellow(),
                    };
                    println!(
                        "{:<20} {}",
                        r["slug"].as_str().unwrap_or("N/A").cyan(),
                        status_colored,
                    );
                }
                println!();
                println!("{} サーバー更新済み", updated.to_string().green());
            } else if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
            }
        }
        ServerCommands::Ping { hostname } => {
            println!("{} {}", "Tailscale ping:".bold(), hostname.cyan());
            println!();

            let resp =
                cp_client::request(&client, "server", "ping", json!({ "hostname": hostname }))
                    .await?;

            if let Some(true) = resp["reachable"].as_bool() {
                let latency = resp["latency_ms"]
                    .as_f64()
                    .map(|l| format!("{l:.1}ms"))
                    .unwrap_or_else(|| "N/A".to_string());
                let via = resp["via"].as_str().unwrap_or("N/A");
                println!("  到達可能: {}", "YES".green());
                println!("  レイテンシ: {}", latency.cyan());
                println!("  経路: {}", via);
            } else {
                println!("  到達可能: {}", "NO".red());
                if let Some(err) = resp["error"].as_str() {
                    println!("  エラー: {}", err);
                }
            }
        }
    }

    client.disconnect().await.ok();
    Ok(())
}

pub async fn handle_cost(cmd: &CostCommands) -> Result<()> {
    let (client, _creds) = cp_client::connect().await?;

    match cmd {
        CostCommands::List { month } => {
            println!("{} {}", "月次コスト:".bold(), month.cyan());
            println!();

            let resp = cp_client::request(
                &client,
                "cost",
                "list",
                json!({ "tenant_slug": "default", "month": month }),
            )
            .await?;

            if let Some(entries) = resp["entries"].as_array() {
                if entries.is_empty() {
                    println!("{}", "コストエントリがありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!(
                            "{:<15} {:<12} {:<15} {}",
                            "PROVIDER", "AMOUNT(¥)", "PROJECT", "DESCRIPTION"
                        )
                        .bold()
                    );
                    println!("{}", "─".repeat(70).dimmed());
                    for e in entries {
                        let amount = e["amount_jpy"].as_i64().unwrap_or(0);
                        println!(
                            "{:<15} {:<12} {:<15} {}",
                            e["provider"].as_str().unwrap_or("N/A"),
                            format!("¥{}", amount),
                            e["project"].as_str().unwrap_or("-"),
                            e["description"].as_str().unwrap_or(""),
                        );
                    }
                }
            }
        }
        CostCommands::Summary { month } => {
            println!("{} {}", "コスト集計:".bold(), month.cyan());
            println!();

            let resp = cp_client::request(
                &client,
                "cost",
                "summary",
                json!({ "tenant_slug": "default", "month": month }),
            )
            .await?;

            if let Some(summaries) = resp["summaries"].as_array() {
                if summaries.is_empty() {
                    println!("{}", "集計データがありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!("{:<15} {:<15} {}", "PROVIDER", "PROJECT", "TOTAL(¥)").bold()
                    );
                    println!("{}", "─".repeat(45).dimmed());
                    let mut grand_total: i64 = 0;
                    for s in summaries {
                        let total = s["total_jpy"].as_i64().unwrap_or(0);
                        grand_total += total;
                        println!(
                            "{:<15} {:<15} {}",
                            s["provider"].as_str().unwrap_or("N/A"),
                            s["project_slug"].as_str().unwrap_or("-"),
                            format!("¥{}", total).cyan(),
                        );
                    }
                    println!("{}", "─".repeat(45).dimmed());
                    println!(
                        "{:<31} {}",
                        "合計".bold(),
                        format!("¥{}", grand_total).green().bold(),
                    );
                }
            }
        }
        CostCommands::Record {
            provider,
            amount,
            month,
            description,
            project,
            stage,
        } => {
            println!("{}", "コストエントリ登録".bold());

            let mut payload = json!({
                "tenant_slug": "default",
                "provider": provider,
                "amount_jpy": amount,
                "month": month,
                "description": description,
            });
            if let Some(proj) = project {
                payload["project_slug"] = json!(proj);
            }
            if let Some(stg) = stage {
                payload["stage"] = json!(stg);
            }

            let resp = cp_client::request(&client, "cost", "record", payload).await?;

            if resp.get("cost_entry").is_some() {
                println!(
                    "{} {} ¥{} ({})",
                    "登録完了:".green(),
                    provider.cyan(),
                    amount,
                    month,
                );
            }
        }
    }

    client.disconnect().await.ok();
    Ok(())
}

pub async fn handle_dns(cmd: &DnsCommands) -> Result<()> {
    let (client, _creds) = cp_client::connect().await?;

    match cmd {
        DnsCommands::List => {
            println!("{}", "DNS レコード一覧".bold());
            println!();

            let resp =
                cp_client::request(&client, "dns", "list", json!({ "tenant_slug": "default" }))
                    .await?;

            if let Some(records) = resp["dns_records"].as_array() {
                if records.is_empty() {
                    println!("{}", "DNS レコードがありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!(
                            "{:<30} {:<8} {:<30} {}",
                            "NAME", "TYPE", "CONTENT", "PROXIED"
                        )
                        .bold()
                    );
                    println!("{}", "─".repeat(75).dimmed());
                    for r in records {
                        let proxied = if r["proxied"].as_bool().unwrap_or(false) {
                            "yes".green().to_string()
                        } else {
                            "no".dimmed().to_string()
                        };
                        println!(
                            "{:<30} {:<8} {:<30} {}",
                            r["name"].as_str().unwrap_or("N/A").cyan(),
                            r["record_type"].as_str().unwrap_or("N/A"),
                            r["content"].as_str().unwrap_or("N/A"),
                            proxied,
                        );
                    }
                }
            }
        }
        DnsCommands::Create {
            name,
            record_type,
            content,
            proxied,
            project,
        } => {
            println!("{}", "DNS レコード作成".bold());

            let mut payload = json!({
                "tenant_slug": "default",
                "name": name,
                "record_type": record_type,
                "content": content,
                "proxied": proxied,
            });
            if let Some(proj) = project {
                payload["project_slug"] = json!(proj);
            }

            let resp = cp_client::request(&client, "dns", "create", payload).await?;

            if resp.get("dns_record").is_some() {
                println!(
                    "{} {} {} → {}",
                    "作成完了:".green(),
                    record_type,
                    name.cyan(),
                    content
                );
            }
        }
        DnsCommands::Delete { name } => {
            println!("{}", "DNS レコード削除".bold());

            let resp =
                cp_client::request(&client, "dns", "delete", json!({ "name": name })).await?;

            if resp["deleted"].as_bool().unwrap_or(false) {
                println!("{} {}", "削除完了:".green(), name.cyan());
            } else {
                println!("{}", "レコードが見つかりません。".yellow());
            }
        }
        DnsCommands::Sync => {
            println!("{}", "Cloudflare DNS 同期".bold());

            let resp =
                cp_client::request(&client, "dns", "sync", json!({ "tenant_slug": "default" }))
                    .await?;

            if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
            } else {
                let imported = resp["imported"].as_u64().unwrap_or(0);
                let cf_total = resp["cf_total"].as_u64().unwrap_or(0);
                let db_total = resp["db_total"].as_u64().unwrap_or(0);

                println!("{} Cloudflare: {} レコード", "CF:".cyan(), cf_total);
                println!("{} DB: {} レコード", "DB:".cyan(), db_total);
                println!("{} インポート: {}", "結果:".green(), imported);

                if let Some(not_in_cf) = resp["not_in_cloudflare"].as_array()
                    && !not_in_cf.is_empty()
                {
                    println!("\n{} Cloudflare に存在しないレコード:", "注意:".yellow());
                    for name in not_in_cf {
                        println!("  - {}", name.as_str().unwrap_or("-"));
                    }
                }
            }
        }
    }

    client.disconnect().await.ok();
    Ok(())
}

pub async fn handle_remote(cmd: &RemoteCommands) -> Result<()> {
    let (client, _creds) = cp_client::connect().await?;

    match cmd {
        RemoteCommands::Deploy {
            project,
            stage,
            server,
            command,
        } => {
            println!(
                "{} {} → {} ({})",
                "リモートデプロイ:".bold(),
                project.cyan(),
                server.cyan(),
                stage
            );
            println!("  コマンド: {}", command.dimmed());
            println!();

            let resp = cp_client::request(
                &client,
                "deploy",
                "run",
                json!({
                    "tenant_slug": "default",
                    "project_slug": project,
                    "stage": stage,
                    "server_slug": server,
                    "command": command,
                }),
            )
            .await?;

            let status = resp["status"].as_str().unwrap_or("unknown");
            let status_colored = match status {
                "success" => status.green(),
                "failed" => status.red(),
                _ => status.yellow(),
            };
            println!("  結果: {}", status_colored);

            if let Some(log) = resp["log"].as_str()
                && !log.is_empty()
            {
                println!();
                println!("{}", "─ ログ ─".dimmed());
                for line in log.lines().take(50) {
                    println!("  {}", line);
                }
            }

            if let Some(err) = resp["error"].as_str() {
                println!("  {}: {}", "エラー".red(), err);
            }
        }
        RemoteCommands::History { limit } => {
            println!("{}", "デプロイ履歴".bold());
            println!();

            let resp = cp_client::request(
                &client,
                "deploy",
                "history",
                json!({
                    "tenant_slug": "default",
                    "limit": limit,
                }),
            )
            .await?;

            if let Some(deployments) = resp["deployments"].as_array() {
                if deployments.is_empty() {
                    println!("{}", "デプロイ履歴がありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!(
                            "{:<12} {:<10} {:<15} {:<10} {:<20}",
                            "STAGE", "SERVER", "COMMAND", "STATUS", "STARTED"
                        )
                        .bold()
                    );
                    println!("{}", "─".repeat(67).dimmed());
                    for d in deployments {
                        let status = d["status"].as_str().unwrap_or("unknown");
                        let status_colored = match status {
                            "success" => status.green(),
                            "failed" => status.red(),
                            "running" => status.yellow(),
                            _ => status.dimmed(),
                        };
                        let cmd = d["command"].as_str().unwrap_or("-");
                        let cmd_short = if cmd.len() > 14 {
                            format!("{}…", &cmd[..13])
                        } else {
                            cmd.to_string()
                        };
                        println!(
                            "{:<12} {:<10} {:<15} {:<10} {:<20}",
                            d["stage"].as_str().unwrap_or("-"),
                            d["server_slug"].as_str().unwrap_or("-"),
                            cmd_short,
                            status_colored,
                            d["started_at"].as_str().unwrap_or("-"),
                        );
                    }
                }
            } else if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
            }
        }
    }

    client.disconnect().await.ok();
    Ok(())
}

/// Persistence Volume コマンド (Tier P-2, 2026-04-23)
pub async fn handle_volume(cmd: &VolumeCommands) -> Result<()> {
    match cmd {
        VolumeCommands::List => {
            let (client, creds) = cp_client::connect().await?;

            println!("{}", "Volume 一覧".bold());
            println!();

            let tenant_slug = creds.tenant_slug.as_deref().unwrap_or("default");
            let resp = cp_client::request(
                &client,
                "volume",
                "list",
                json!({ "tenant_slug": tenant_slug }),
            )
            .await?;

            if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
                client.disconnect().await.ok();
                return Ok(());
            }

            if let Some(volumes) = resp["volumes"].as_array() {
                if volumes.is_empty() {
                    println!("{}", "Volume がありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!(
                            "{:<20} {:<16} {:<30} {:<10} {:<6}",
                            "SLUG", "TIER", "MOUNT", "STATE", "BYO"
                        )
                        .bold()
                    );
                    println!("{}", "─".repeat(88).dimmed());
                    for v in volumes {
                        println!(
                            "{:<20} {:<16} {:<30} {:<10} {:<6}",
                            v["slug"].as_str().unwrap_or("-").cyan(),
                            v["tier"].as_str().unwrap_or("-"),
                            v["mount"].as_str().unwrap_or("-").dimmed(),
                            v["state"].as_str().unwrap_or("-"),
                            if v["bring_your_own"].as_bool().unwrap_or(false) {
                                "yes".yellow().to_string()
                            } else {
                                "no".dimmed().to_string()
                            },
                        );
                    }
                }
            }

            client.disconnect().await.ok();
        }
        VolumeCommands::Adopt {
            slug,
            server,
            mount,
            tier,
        } => {
            let (client, creds) = cp_client::connect().await?;

            println!("{}", "Volume adopt (BYO)".bold());
            println!("  データには一切触れません。CP registry に record を作成します。");
            println!();

            let tenant_slug = creds.tenant_slug.as_deref().unwrap_or("default");
            let resp = cp_client::request(
                &client,
                "volume",
                "adopt",
                json!({
                    "tenant_slug": tenant_slug,
                    "server_slug": server,
                    "slug": slug,
                    "mount": mount,
                    "tier": tier,
                }),
            )
            .await?;

            if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
            } else if let Some(volume) = resp.get("volume") {
                println!("{}", "Adopted 🎉".green().bold());
                println!(
                    "  Slug:   {}",
                    volume["slug"].as_str().unwrap_or("N/A").cyan()
                );
                println!("  Tier:   {}", volume["tier"].as_str().unwrap_or("N/A"));
                println!(
                    "  Mount:  {}",
                    volume["mount"].as_str().unwrap_or("N/A").dimmed()
                );
                println!("  Server: {}", server.cyan());
                println!(
                    "  BYO:    {}",
                    if volume["bring_your_own"].as_bool().unwrap_or(false) {
                        "yes".yellow().to_string()
                    } else {
                        "no".dimmed().to_string()
                    }
                );
            }

            client.disconnect().await.ok();
        }
    }

    Ok(())
}

/// Build Tier コマンド (v1 MVP, 2026-04-23)
pub async fn handle_build(cmd: &BuildCommands) -> Result<()> {
    match cmd {
        BuildCommands::Submit {
            git,
            git_ref,
            dockerfile,
            image,
            kind,
            project: _,
        } => {
            let (client, creds) = cp_client::connect().await?;

            println!("{}", "Build Job Submit".bold());
            println!();

            let tenant_slug = creds.tenant_slug.as_deref().unwrap_or("default");
            let mut payload = json!({
                "tenant_slug": tenant_slug,
                "git_url": git,
                "git_ref": git_ref,
                "kind": kind,
            });
            if let Some(df) = dockerfile {
                payload["dockerfile"] = serde_json::Value::String(df.clone());
            }
            if let Some(img) = image {
                payload["image"] = serde_json::Value::String(img.clone());
            }

            let resp = cp_client::request(&client, "build", "submit", payload).await?;

            if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
            } else if let Some(job) = resp.get("build_job") {
                println!("{}", "Submitted".green().bold());
                println!("  ID:      {}", format!("{:?}", job["id"]).cyan());
                println!("  Kind:    {}", job["kind"].as_str().unwrap_or("-"));
                println!(
                    "  State:   {}",
                    job["state"].as_str().unwrap_or("-").yellow()
                );
                println!(
                    "  Git URL: {}",
                    job["source"]["git_url"].as_str().unwrap_or("-").dimmed()
                );
                println!(
                    "  Ref:     {}",
                    job["source"]["git_ref"].as_str().unwrap_or("-")
                );
            }

            client.disconnect().await.ok();
        }
        BuildCommands::List => {
            let (client, creds) = cp_client::connect().await?;

            println!("{}", "Build Job 一覧".bold());
            println!();

            let tenant_slug = creds.tenant_slug.as_deref().unwrap_or("default");
            let resp = cp_client::request(
                &client,
                "build",
                "list",
                json!({ "tenant_slug": tenant_slug }),
            )
            .await?;

            if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
                client.disconnect().await.ok();
                return Ok(());
            }

            if let Some(jobs) = resp["build_jobs"].as_array() {
                if jobs.is_empty() {
                    println!("{}", "Build Job がありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!(
                            "{:<36} {:<14} {:<10} {:<40}",
                            "ID", "KIND", "STATE", "GIT URL"
                        )
                        .bold()
                    );
                    println!("{}", "─".repeat(104).dimmed());
                    for j in jobs {
                        println!(
                            "{:<36} {:<14} {:<10} {:<40}",
                            format!("{:?}", j["id"]).cyan(),
                            j["kind"].as_str().unwrap_or("-"),
                            j["state"].as_str().unwrap_or("-").yellow(),
                            j["source"]["git_url"].as_str().unwrap_or("-").dimmed(),
                        );
                    }
                }
            }

            client.disconnect().await.ok();
        }
        BuildCommands::Show { id } => {
            let (client, creds) = cp_client::connect().await?;

            let tenant_slug = creds.tenant_slug.as_deref().unwrap_or("default");
            let resp = cp_client::request(
                &client,
                "build",
                "get",
                json!({ "tenant_slug": tenant_slug, "job_id": id }),
            )
            .await?;

            if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
            } else if let Some(job) = resp.get("build_job") {
                println!("{}", "Build Job 詳細".bold());
                println!("  ID:              {}", format!("{:?}", job["id"]).cyan());
                println!("  Kind:            {}", job["kind"].as_str().unwrap_or("-"));
                println!(
                    "  State:           {}",
                    job["state"].as_str().unwrap_or("-").yellow()
                );
                println!(
                    "  Git URL:         {}",
                    job["source"]["git_url"].as_str().unwrap_or("-").dimmed()
                );
                println!(
                    "  Ref:             {}",
                    job["source"]["git_ref"].as_str().unwrap_or("-")
                );
                if let Some(df) = job["source"]["dockerfile"].as_str() {
                    println!("  Dockerfile:      {}", df);
                }
                if let Some(img) = job["target"]["image"].as_str() {
                    println!("  Image:           {}", img);
                }
                if let Some(logs) = job["logs_url"].as_str() {
                    println!("  Logs URL:        {}", logs.dimmed());
                }
                if let Some(dur) = job["duration_seconds"].as_i64() {
                    println!("  Duration:        {}s", dur);
                }
            }

            client.disconnect().await.ok();
        }
        BuildCommands::Logs { id } => {
            let (client, creds) = cp_client::connect().await?;

            let tenant_slug = creds.tenant_slug.as_deref().unwrap_or("default");
            let resp = cp_client::request(
                &client,
                "build",
                "get",
                json!({ "tenant_slug": tenant_slug, "job_id": id }),
            )
            .await?;

            if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
            } else if let Some(job) = resp.get("build_job") {
                println!("{}", "Build Logs".bold());
                println!(
                    "  State:    {}",
                    job["state"].as_str().unwrap_or("-").yellow()
                );
                if let Some(logs_url) = job["logs_url"].as_str() {
                    println!("  Logs URL: {}", logs_url.dimmed());
                    println!();
                    println!(
                        "{}",
                        "v1: logs_url を polling して取得してください。".dimmed()
                    );
                } else {
                    println!(
                        "  {}",
                        "ログ URL はまだ利用できません (build 開始後に設定されます)。".dimmed()
                    );
                }
            }

            client.disconnect().await.ok();
        }
        BuildCommands::Cancel { id } => {
            let (client, creds) = cp_client::connect().await?;

            let tenant_slug = creds.tenant_slug.as_deref().unwrap_or("default");
            let resp = cp_client::request(
                &client,
                "build",
                "cancel",
                json!({ "tenant_slug": tenant_slug, "job_id": id }),
            )
            .await?;

            if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
            } else {
                println!(
                    "{} Build Job {} をキャンセルしました。",
                    "Cancelled.".green().bold(),
                    id.cyan()
                );
            }

            client.disconnect().await.ok();
        }
    }

    Ok(())
}

/// Stage Tier コマンド (FSC-16, 2026-04-24)
///
/// 既存稼働中の stage を非破壊で CP registry に adopt する。
/// provision phase は FSC-31 で別途実装予定。
pub async fn handle_stage(cmd: &StageCommands) -> Result<()> {
    match cmd {
        StageCommands::Adopt {
            project,
            project_name,
            stage,
            description,
            server,
            services,
        } => {
            if services.is_empty() {
                anyhow::bail!("少なくとも 1 つの --service slug=image を指定してください");
            }

            let (client, creds) = cp_client::connect().await?;

            println!("{}", "Stage adopt (BYO)".bold());
            println!("  docker 状態には一切触れません。CP registry に record を作成します。");
            println!();

            let tenant_slug = creds.tenant_slug.as_deref().unwrap_or("default");

            let services_payload: Vec<_> = services
                .iter()
                .map(|s| json!({ "slug": s.slug, "image": s.image }))
                .collect();

            let mut payload = json!({
                "tenant_slug": tenant_slug,
                "server_slug": server,
                "project_slug": project,
                "stage_slug": stage,
                "services": services_payload,
            });
            if let Some(name) = project_name {
                payload["project_name"] = json!(name);
            }
            if let Some(desc) = description {
                payload["description"] = json!(desc);
            }

            let resp = cp_client::request(&client, "stage", "adopt", payload).await?;

            if let Some(err) = resp["error"].as_str() {
                println!("{} {}", "エラー:".red(), err);
            } else if let Some(outcome) = resp.get("outcome") {
                println!("{}", "Adopted 🎉".green().bold());
                if let Some(p) = outcome.get("project") {
                    println!("  Project: {}", p["slug"].as_str().unwrap_or("-").cyan());
                }
                if let Some(st) = outcome.get("stage") {
                    println!("  Stage:   {}", st["slug"].as_str().unwrap_or("-").cyan());
                }
                println!("  Server:  {}", server.cyan());
                if let Some(svcs) = outcome["services"].as_array() {
                    println!("  Services ({}):", svcs.len());
                    for s in svcs {
                        println!(
                            "    - {}  ({})",
                            s["slug"].as_str().unwrap_or("-").cyan(),
                            s["image"].as_str().unwrap_or("-").dimmed()
                        );
                    }
                }
            }

            client.disconnect().await.ok();
        }
    }

    Ok(())
}
