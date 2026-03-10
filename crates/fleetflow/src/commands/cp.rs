//! Control Plane リソース管理コマンド
//!
//! fleet tenant / fleet project / fleet server の各サブコマンドを処理する。
//! Unison Protocol 経由で CP サーバーに接続してリクエストを送信する。

use anyhow::Result;
use colored::Colorize;
use serde_json::json;

use super::cp_client;
use crate::{CostCommands, DnsCommands, ProjectCommands, ServerCommands, TenantCommands};

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
    }

    client.disconnect().await.ok();
    Ok(())
}
