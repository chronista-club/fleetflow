//! Control Plane リソース管理コマンド
//!
//! fleet tenant / fleet project / fleet server の各サブコマンドを処理する。
//! Unison Protocol 経由で CP サーバーに接続してリクエストを送信する。

use anyhow::Result;
use colored::Colorize;
use serde_json::json;

use super::cp_client;
use crate::{ProjectCommands, ServerCommands, TenantCommands};

pub async fn handle_tenant(cmd: &TenantCommands) -> Result<()> {
    match cmd {
        TenantCommands::Status => {
            let (client, creds) = cp_client::connect().await?;

            println!("{}", "テナント状態".bold());
            println!();

            let slug = creds.tenant_slug.as_deref().unwrap_or("default");
            let resp = cp_client::request(
                &client,
                "tenant",
                "get",
                json!({ "slug": slug }),
            )
            .await?;

            if let Some(tenant) = resp.get("tenant") {
                println!("  Name: {}", tenant["name"].as_str().unwrap_or("N/A").cyan());
                println!("  Slug: {}", tenant["slug"].as_str().unwrap_or("N/A").cyan());
            } else {
                println!("{}", "テナント情報が見つかりません。".yellow());
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

            let resp = cp_client::request(
                &client,
                "project",
                "list",
                json!({}),
            )
            .await?;

            if let Some(projects) = resp["projects"].as_array() {
                if projects.is_empty() {
                    println!("{}", "プロジェクトがありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!("{:<25} {:<30} {:<20}", "SLUG", "NAME", "CREATED")
                            .bold()
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

            let resp = cp_client::request(
                &client,
                "project",
                "get",
                json!({ "slug": slug }),
            )
            .await?;

            if let Some(project) = resp.get("project") {
                println!("  Name:        {}", project["name"].as_str().unwrap_or("N/A").cyan());
                println!("  Slug:        {}", project["slug"].as_str().unwrap_or("N/A").cyan());
                println!("  Description: {}", project["description"].as_str().unwrap_or("N/A"));
                println!("  Created:     {}", project["created_at"].as_str().unwrap_or("N/A").dimmed());
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

            let resp = cp_client::request(
                &client,
                "server",
                "list",
                json!({}),
            )
            .await?;

            if let Some(servers) = resp["servers"].as_array() {
                if servers.is_empty() {
                    println!("{}", "サーバーがありません。".dimmed());
                } else {
                    println!(
                        "{}",
                        format!("{:<15} {:<20} {:<15} {:<15} {:<10}", "SLUG", "HOSTNAME", "PROVIDER", "IP", "STATUS")
                            .bold()
                    );
                    println!("{}", "─".repeat(75).dimmed());
                    for s in servers {
                        let status = s["status"].as_str().unwrap_or("unknown");
                        let status_colored = match status {
                            "online" => status.green(),
                            "offline" => status.red(),
                            _ => status.yellow(),
                        };
                        println!(
                            "{:<15} {:<20} {:<15} {:<15} {:<10}",
                            s["slug"].as_str().unwrap_or("N/A").cyan(),
                            s["hostname"].as_str().unwrap_or("N/A"),
                            s["provider"].as_str().unwrap_or("N/A"),
                            s["ip_address"].as_str().unwrap_or("N/A"),
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
                payload["ip_address"] = json!(host);
            }
            if let Some(path) = deploy_path {
                payload["deploy_path"] = json!(path);
            }

            let resp = cp_client::request(
                &client,
                "server",
                "register",
                payload,
            )
            .await?;

            if resp.get("server").is_some() {
                println!("{} {}", "登録完了:".green(), slug.cyan());
            }
        }
        ServerCommands::Status { slug } => {
            println!("{} {}", "サーバー状態:".bold(), slug.cyan());
            println!();

            // server チャネルには get メソッドがないので list から検索
            let resp = cp_client::request(
                &client,
                "server",
                "list",
                json!({}),
            )
            .await?;

            if let Some(servers) = resp["servers"].as_array() {
                if let Some(server) = servers.iter().find(|s| s["slug"].as_str() == Some(slug.as_str())) {
                    println!("  Slug:      {}", server["slug"].as_str().unwrap_or("N/A").cyan());
                    println!("  Hostname:  {}", server["hostname"].as_str().unwrap_or("N/A"));
                    println!("  Provider:  {}", server["provider"].as_str().unwrap_or("N/A"));
                    println!("  IP:        {}", server["ip_address"].as_str().unwrap_or("N/A"));
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
