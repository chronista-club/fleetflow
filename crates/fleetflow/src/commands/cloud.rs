use colored::Colorize;
use fleetflow_cloud::{ActionType, CloudProvider, ResourceConfig, ResourceSet};
use fleetflow_cloud_cloudflare::CloudflareProvider;
use fleetflow_cloud_sakura::SakuraCloudProvider;

use super::super::CloudCommands;

/// Flow からクラウドプロバイダー一覧をインスタンス化
fn create_providers(config: &fleetflow_core::Flow) -> Vec<Box<dyn CloudProvider>> {
    let mut providers: Vec<Box<dyn CloudProvider>> = Vec::new();

    for (name, provider_config) in &config.providers {
        match name.as_str() {
            "sakura-cloud" => {
                let zone = provider_config.zone.as_deref().unwrap_or("tk1a");
                providers.push(Box::new(SakuraCloudProvider::new(zone)));
            }
            "cloudflare" => {
                let account_id = provider_config.config.get("account_id").cloned();
                providers.push(Box::new(CloudflareProvider::new(account_id)));
            }
            other => {
                tracing::warn!("未対応のプロバイダー: {}", other);
            }
        }
    }

    providers
}

/// Flow.servers → ResourceSet に変換
fn build_resource_set(config: &fleetflow_core::Flow, stage_name: &str) -> ResourceSet {
    let mut resource_set = ResourceSet::new();

    // ステージに紐づくサーバーを取得
    let stage_servers: Vec<&String> = if let Some(stage) = config.stages.get(stage_name) {
        stage.servers.iter().collect()
    } else {
        // ステージ未指定の場合、全サーバーを対象
        config.servers.keys().collect()
    };

    for server_name in stage_servers {
        if let Some(server) = config.servers.get(server_name) {
            let mut details = serde_json::Map::new();

            if let Some(ref plan) = server.plan {
                details.insert("plan".to_string(), serde_json::json!(plan));
            }
            if let Some(disk_size) = server.disk_size {
                details.insert("disk_size".to_string(), serde_json::json!(disk_size));
            }
            if let Some(ref os) = server.os {
                details.insert("os".to_string(), serde_json::json!(os));
            }
            if !server.ssh_keys.is_empty() {
                details.insert("ssh_keys".to_string(), serde_json::json!(server.ssh_keys));
            }
            if !server.tags.is_empty() {
                details.insert("tags".to_string(), serde_json::json!(server.tags));
            }
            if let Some(ref script) = server.startup_script {
                details.insert("startup_scripts".to_string(), serde_json::json!([script]));
            }

            resource_set.add(ResourceConfig::new(
                "server",
                server_name.clone(),
                server.provider.clone(),
                serde_json::Value::Object(details),
            ));

            // DNS レコードを ResourceConfig として追加（provider は "cloudflare"）
            let dns_hostname = server.config.get("dns_hostname");
            if let Some(hostname) = dns_hostname {
                // A レコード: hostname → サーバーIP（apply 時に解決）
                let dns_details = serde_json::json!({
                    "record_type": "A",
                    "hostname": hostname,
                    "server_name": server_name,
                });
                resource_set.add(ResourceConfig::new(
                    "dns-record",
                    format!("dns-a-{}", hostname),
                    "cloudflare".to_string(),
                    dns_details,
                ));

                // CNAME レコード: 各 alias → hostname
                for alias in &server.dns_aliases {
                    let cname_details = serde_json::json!({
                        "record_type": "CNAME",
                        "hostname": alias,
                        "target": hostname,
                        "server_name": server_name,
                    });
                    resource_set.add(ResourceConfig::new(
                        "dns-record",
                        format!("dns-cname-{}", alias),
                        "cloudflare".to_string(),
                        cname_details,
                    ));
                }
            }
        }
    }

    resource_set
}

/// plan の結果を Terraform ライクに表示
fn print_plan(plan: &fleetflow_cloud::Plan) {
    if !plan.has_changes {
        println!();
        println!("{}", "変更はありません。インフラは最新の状態です。".green());
        return;
    }

    println!();
    println!("{}", "FleetFlow は以下の変更を計画しています:".bold());
    println!();

    for action in &plan.actions {
        let (symbol, color_fn): (&str, fn(&str) -> colored::ColoredString) =
            match action.action_type {
                ActionType::Create => ("+", |s: &str| s.green()),
                ActionType::Update => ("~", |s: &str| s.yellow()),
                ActionType::Delete => ("-", |s: &str| s.red()),
                ActionType::NoOp => continue,
            };

        println!(
            "  {} {} {} ({})",
            color_fn(symbol),
            color_fn(&action.resource_type),
            color_fn(&action.resource_id),
            action.description
        );

        for (key, value) in &action.details {
            println!("      {}: {}", key.dimmed(), value);
        }
    }

    let summary = plan.summary();
    println!();
    println!(
        "Plan: {} to create, {} to update, {} to delete",
        summary.create.to_string().green(),
        summary.update.to_string().yellow(),
        summary.delete.to_string().red(),
    );
}

/// apply の結果を表示
fn print_apply_result(result: &fleetflow_cloud::ApplyResult) {
    println!();
    for s in &result.succeeded {
        println!("  {} {}", "✓".green(), s.message);
    }
    for f in &result.failed {
        println!(
            "  {} {}",
            "✗".red(),
            f.error.as_deref().unwrap_or("不明なエラー")
        );
    }
    println!();
    println!(
        "結果: {} 成功, {} 失敗 ({}ms)",
        result.succeeded.len().to_string().green(),
        result.failed.len().to_string().red(),
        result.duration_ms,
    );
}

pub async fn handle(cmd: CloudCommands, config: &fleetflow_core::Flow) -> anyhow::Result<()> {
    match cmd {
        CloudCommands::Auth => {
            println!("{}", "クラウドプロバイダーの認証状態を確認中...".blue());

            let providers = create_providers(config);

            if providers.is_empty() {
                println!();
                println!(
                    "{}",
                    "プロバイダーが設定されていません。fleet.kdl に providers ブロックを追加してください。"
                        .yellow()
                );
                return Ok(());
            }

            for provider in &providers {
                println!();
                println!(
                    "  {} ({})",
                    provider.display_name().bold(),
                    provider.name().dimmed()
                );

                match provider.check_auth().await {
                    Ok(auth) if auth.authenticated => {
                        println!(
                            "    {} {}",
                            "✓".green(),
                            auth.account_info.unwrap_or_default()
                        );
                    }
                    Ok(auth) => {
                        println!(
                            "    {} {}",
                            "✗".red(),
                            auth.error.unwrap_or_else(|| "認証失敗".to_string())
                        );
                    }
                    Err(e) => {
                        println!("    {} {}", "✗".red(), e);
                    }
                }
            }
        }

        CloudCommands::Plan { stage } => {
            let stage_name = crate::utils::determine_stage_name(Some(stage), config)?;
            println!(
                "{}",
                format!("ステージ '{}' のインフラ計画を作成中...", stage_name)
                    .blue()
                    .bold()
            );

            let providers = create_providers(config);
            if providers.is_empty() {
                anyhow::bail!("プロバイダーが設定されていません");
            }

            let resource_set = build_resource_set(config, &stage_name);
            if resource_set.resources.is_empty() {
                println!();
                println!(
                    "{}",
                    format!(
                        "ステージ '{}' にクラウドリソースが定義されていません。",
                        stage_name
                    )
                    .yellow()
                );
                return Ok(());
            }

            // 各プロバイダーで plan を実行
            for provider in &providers {
                let provider_resources: ResourceSet = {
                    let mut set = ResourceSet::new();
                    for r in resource_set.iter() {
                        if r.provider == provider.name() {
                            set.add(r.clone());
                        }
                    }
                    set
                };

                if provider_resources.resources.is_empty() {
                    continue;
                }

                println!();
                println!(
                    "{} ({}):",
                    provider.display_name().bold(),
                    provider.name().dimmed()
                );

                match provider.plan(&provider_resources).await {
                    Ok(plan) => print_plan(&plan),
                    Err(e) => {
                        println!("  {} plan 失敗: {}", "✗".red(), e);
                    }
                }
            }
        }

        CloudCommands::Up { stage, yes } => {
            let stage_name = crate::utils::determine_stage_name(Some(stage), config)?;

            if !yes {
                println!(
                    "{}",
                    format!(
                        "⚠ ステージ '{}' のクラウドリソースを作成します。",
                        stage_name
                    )
                    .yellow()
                );
                println!("実行するには --yes オプションを指定してください");
                std::process::exit(2);
            }

            println!(
                "{}",
                format!("ステージ '{}' のクラウドリソースを構築中...", stage_name)
                    .blue()
                    .bold()
            );

            let providers = create_providers(config);
            if providers.is_empty() {
                anyhow::bail!("プロバイダーが設定されていません");
            }

            let resource_set = build_resource_set(config, &stage_name);

            for provider in &providers {
                let provider_resources: ResourceSet = {
                    let mut set = ResourceSet::new();
                    for r in resource_set.iter() {
                        if r.provider == provider.name() {
                            set.add(r.clone());
                        }
                    }
                    set
                };

                if provider_resources.resources.is_empty() {
                    continue;
                }

                println!();
                println!(
                    "{} ({}):",
                    provider.display_name().bold(),
                    provider.name().dimmed()
                );

                // plan → apply
                let plan = provider.plan(&provider_resources).await?;

                if !plan.has_changes {
                    println!("  {}", "変更なし。インフラは最新の状態です。".green());
                    continue;
                }

                print_plan(&plan);
                println!();
                println!("{}", "適用中...".blue());

                match provider.apply(&plan).await {
                    Ok(result) => {
                        print_apply_result(&result);
                        if !result.is_success() {
                            anyhow::bail!("一部のリソース作成に失敗しました");
                        }
                    }
                    Err(e) => {
                        anyhow::bail!("apply 失敗: {}", e);
                    }
                }
            }

            println!();
            println!(
                "{}",
                format!(
                    "✓ ステージ '{}' のクラウドリソースを構築しました",
                    stage_name
                )
                .green()
                .bold()
            );
        }

        CloudCommands::Down { stage, yes } => {
            let stage_name = crate::utils::determine_stage_name(Some(stage), config)?;

            if !yes {
                println!(
                    "{}",
                    format!(
                        "⚠ ステージ '{}' のクラウドリソースを削除します。",
                        stage_name
                    )
                    .red()
                    .bold()
                );
                println!("{}", "データは復旧できません。".red());
                println!("実行するには --yes オプションを指定してください");
                std::process::exit(2);
            }

            println!(
                "{}",
                format!("ステージ '{}' のクラウドリソースを削除中...", stage_name)
                    .red()
                    .bold()
            );

            let providers = create_providers(config);
            let resource_set = build_resource_set(config, &stage_name);

            for provider in &providers {
                let provider_resources: ResourceSet = {
                    let mut set = ResourceSet::new();
                    for r in resource_set.iter() {
                        if r.provider == provider.name() {
                            set.add(r.clone());
                        }
                    }
                    set
                };

                if provider_resources.resources.is_empty() {
                    continue;
                }

                println!();
                println!(
                    "{} ({}):",
                    provider.display_name().bold(),
                    provider.name().dimmed()
                );

                for resource in provider_resources.iter() {
                    println!(
                        "  {} {} {}",
                        "-".red(),
                        resource.resource_type.red(),
                        resource.id.red()
                    );

                    match provider.destroy(&resource.id).await {
                        Ok(()) => {
                            println!("    {} {} を削除しました", "✓".green(), resource.id);
                        }
                        Err(e) => {
                            println!("    {} 削除失敗: {}", "✗".red(), e);
                        }
                    }
                }
            }

            println!();
            println!(
                "{}",
                format!(
                    "✓ ステージ '{}' のクラウドリソースを削除しました",
                    stage_name
                )
                .red()
                .bold()
            );
        }

        CloudCommands::Status { stage } => {
            let stage_name = crate::utils::determine_stage_name(stage, config)?;
            println!(
                "{}",
                format!("ステージ '{}' のクラウドリソース状態:", stage_name)
                    .blue()
                    .bold()
            );

            let providers = create_providers(config);

            if providers.is_empty() {
                println!();
                println!("{}", "プロバイダーが設定されていません。".yellow());
                return Ok(());
            }

            for provider in &providers {
                println!();
                println!(
                    "{} ({}):",
                    provider.display_name().bold(),
                    provider.name().dimmed()
                );

                match provider.get_state().await {
                    Ok(state) => {
                        if state.is_empty() {
                            println!("  {}", "リソースなし".dimmed());
                            continue;
                        }

                        for (name, resource) in state.iter() {
                            let status_str = format!("{:?}", resource.status);
                            let status_colored = match resource.status {
                                fleetflow_cloud::ResourceStatus::Running => status_str.green(),
                                fleetflow_cloud::ResourceStatus::Stopped => status_str.yellow(),
                                _ => status_str.dimmed(),
                            };

                            println!("  {} {} - {}", "•".cyan(), name.bold(), status_colored);

                            // IP アドレスがあれば表示
                            if let Some(ip) = resource.attributes.get("ip") {
                                println!("    IP: {}", ip);
                            }
                        }
                    }
                    Err(e) => {
                        println!("  {} 状態取得失敗: {}", "✗".red(), e);
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetflow_core::model::{CloudProvider as CloudProviderModel, ServerResource, Stage};
    use std::collections::HashMap;

    fn make_flow_with_cloud() -> fleetflow_core::Flow {
        let mut providers = HashMap::new();
        providers.insert(
            "sakura-cloud".to_string(),
            CloudProviderModel {
                name: "sakura-cloud".to_string(),
                zone: Some("tk1a".to_string()),
                config: HashMap::new(),
            },
        );

        let mut servers = HashMap::new();
        servers.insert(
            "web-01".to_string(),
            ServerResource {
                provider: "sakura-cloud".to_string(),
                plan: Some("2core-4gb".to_string()),
                disk_size: Some(100),
                os: Some("ubuntu-24.04".to_string()),
                ssh_keys: vec!["my-key".to_string()],
                ..Default::default()
            },
        );
        servers.insert(
            "db-01".to_string(),
            ServerResource {
                provider: "sakura-cloud".to_string(),
                plan: Some("4core-8gb".to_string()),
                disk_size: Some(200),
                ..Default::default()
            },
        );

        let mut stages = HashMap::new();
        stages.insert(
            "dev".to_string(),
            Stage {
                services: vec!["app".to_string()],
                servers: vec!["web-01".to_string()],
                ..Default::default()
            },
        );
        stages.insert(
            "prod".to_string(),
            Stage {
                services: vec!["app".to_string()],
                servers: vec!["web-01".to_string(), "db-01".to_string()],
                ..Default::default()
            },
        );

        fleetflow_core::Flow {
            name: "test-project".to_string(),
            providers,
            servers,
            stages,
            services: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
        }
    }

    #[test]
    fn test_create_providers_sakura() {
        let flow = make_flow_with_cloud();
        let providers = create_providers(&flow);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].name(), "sakura-cloud");
    }

    #[test]
    fn test_create_providers_empty() {
        let flow = fleetflow_core::Flow {
            name: "empty".to_string(),
            providers: HashMap::new(),
            servers: HashMap::new(),
            stages: HashMap::new(),
            services: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
        };
        let providers = create_providers(&flow);
        assert!(providers.is_empty());
    }

    #[test]
    fn test_create_providers_unknown_skipped() {
        let mut providers_map = HashMap::new();
        providers_map.insert(
            "aws".to_string(),
            CloudProviderModel {
                name: "aws".to_string(),
                zone: None,
                config: HashMap::new(),
            },
        );
        let flow = fleetflow_core::Flow {
            name: "test".to_string(),
            providers: providers_map,
            servers: HashMap::new(),
            stages: HashMap::new(),
            services: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
        };
        let providers = create_providers(&flow);
        assert!(providers.is_empty());
    }

    #[test]
    fn test_build_resource_set_dev_stage() {
        let flow = make_flow_with_cloud();
        let resource_set = build_resource_set(&flow, "dev");

        // dev ステージは web-01 のみ
        assert_eq!(resource_set.resources.len(), 1);
        let web = resource_set.get("server", "web-01");
        assert!(web.is_some());
        assert_eq!(web.unwrap().provider, "sakura-cloud");
    }

    #[test]
    fn test_build_resource_set_prod_stage() {
        let flow = make_flow_with_cloud();
        let resource_set = build_resource_set(&flow, "prod");

        // prod ステージは web-01 + db-01
        assert_eq!(resource_set.resources.len(), 2);
        assert!(resource_set.get("server", "web-01").is_some());
        assert!(resource_set.get("server", "db-01").is_some());
    }

    #[test]
    fn test_build_resource_set_unknown_stage() {
        let flow = make_flow_with_cloud();
        let resource_set = build_resource_set(&flow, "nonexistent");

        // 未知のステージは全サーバーが対象
        assert_eq!(resource_set.resources.len(), 2);
    }

    #[test]
    fn test_build_resource_set_config_values() {
        let flow = make_flow_with_cloud();
        let resource_set = build_resource_set(&flow, "dev");

        let web = resource_set.get("server", "web-01").unwrap();
        assert_eq!(
            web.get_config::<String>("plan"),
            Some("2core-4gb".to_string())
        );
        assert_eq!(web.get_config::<u32>("disk_size"), Some(100));
        assert_eq!(
            web.get_config::<String>("os"),
            Some("ubuntu-24.04".to_string())
        );
    }

    #[test]
    fn test_create_providers_cloudflare() {
        let mut providers_map = HashMap::new();
        providers_map.insert(
            "cloudflare".to_string(),
            CloudProviderModel {
                name: "cloudflare".to_string(),
                zone: None,
                config: HashMap::new(),
            },
        );
        let flow = fleetflow_core::Flow {
            name: "test".to_string(),
            providers: providers_map,
            servers: HashMap::new(),
            stages: HashMap::new(),
            services: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
        };
        let providers = create_providers(&flow);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].name(), "cloudflare");
    }

    #[test]
    fn test_create_providers_both() {
        let mut providers_map = HashMap::new();
        providers_map.insert(
            "sakura-cloud".to_string(),
            CloudProviderModel {
                name: "sakura-cloud".to_string(),
                zone: Some("tk1a".to_string()),
                config: HashMap::new(),
            },
        );
        providers_map.insert(
            "cloudflare".to_string(),
            CloudProviderModel {
                name: "cloudflare".to_string(),
                zone: None,
                config: HashMap::new(),
            },
        );
        let flow = fleetflow_core::Flow {
            name: "test".to_string(),
            providers: providers_map,
            servers: HashMap::new(),
            stages: HashMap::new(),
            services: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
        };
        let providers = create_providers(&flow);
        assert_eq!(providers.len(), 2);
    }

    fn make_flow_with_dns() -> fleetflow_core::Flow {
        let mut providers = HashMap::new();
        providers.insert(
            "sakura-cloud".to_string(),
            CloudProviderModel {
                name: "sakura-cloud".to_string(),
                zone: Some("tk1a".to_string()),
                config: HashMap::new(),
            },
        );

        let mut server_config = HashMap::new();
        server_config.insert("dns_hostname".to_string(), "dev".to_string());

        let mut servers = HashMap::new();
        servers.insert(
            "creo-vps".to_string(),
            ServerResource {
                provider: "sakura-cloud".to_string(),
                plan: Some("4core-8gb".to_string()),
                dns_aliases: vec!["app".to_string(), "api".to_string()],
                config: server_config,
                ..Default::default()
            },
        );

        let mut stages = HashMap::new();
        stages.insert(
            "live".to_string(),
            Stage {
                services: vec![],
                servers: vec!["creo-vps".to_string()],
                ..Default::default()
            },
        );

        fleetflow_core::Flow {
            name: "creo-memories".to_string(),
            providers,
            servers,
            stages,
            services: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
        }
    }

    #[test]
    fn test_build_resource_set_dns_records() {
        let flow = make_flow_with_dns();
        let resource_set = build_resource_set(&flow, "live");

        // server + A レコード + 2 CNAME レコード = 4
        assert_eq!(resource_set.resources.len(), 4);

        // サーバーリソース
        let server = resource_set.get("server", "creo-vps");
        assert!(server.is_some());

        // A レコード
        let a_record = resource_set.get("dns-record", "dns-a-dev");
        assert!(a_record.is_some());
        let a = a_record.unwrap();
        assert_eq!(a.provider, "cloudflare");
        assert_eq!(a.get_config::<String>("record_type"), Some("A".to_string()));
        assert_eq!(a.get_config::<String>("hostname"), Some("dev".to_string()));

        // CNAME レコード
        let cname_app = resource_set.get("dns-record", "dns-cname-app");
        assert!(cname_app.is_some());
        let app = cname_app.unwrap();
        assert_eq!(app.provider, "cloudflare");
        assert_eq!(
            app.get_config::<String>("record_type"),
            Some("CNAME".to_string())
        );
        assert_eq!(app.get_config::<String>("target"), Some("dev".to_string()));

        let cname_api = resource_set.get("dns-record", "dns-cname-api");
        assert!(cname_api.is_some());
    }

    #[test]
    fn test_build_resource_set_no_dns_without_hostname() {
        let mut providers = HashMap::new();
        providers.insert(
            "sakura-cloud".to_string(),
            CloudProviderModel {
                name: "sakura-cloud".to_string(),
                zone: Some("tk1a".to_string()),
                config: HashMap::new(),
            },
        );

        let mut servers = HashMap::new();
        servers.insert(
            "web-01".to_string(),
            ServerResource {
                provider: "sakura-cloud".to_string(),
                plan: Some("2core-4gb".to_string()),
                // dns_hostname なし → DNS レコード生成しない
                ..Default::default()
            },
        );

        let mut stages = HashMap::new();
        stages.insert(
            "dev".to_string(),
            Stage {
                services: vec![],
                servers: vec!["web-01".to_string()],
                ..Default::default()
            },
        );

        let flow = fleetflow_core::Flow {
            name: "test".to_string(),
            providers,
            servers,
            stages,
            services: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
        };

        let resource_set = build_resource_set(&flow, "dev");
        // server のみ、DNS なし
        assert_eq!(resource_set.resources.len(), 1);
        assert!(resource_set.get("server", "web-01").is_some());
        assert_eq!(resource_set.by_type("dns-record").len(), 0);
    }

    #[test]
    fn test_build_resource_set_dns_hostname_only_no_aliases() {
        let mut providers = HashMap::new();
        providers.insert(
            "sakura-cloud".to_string(),
            CloudProviderModel {
                name: "sakura-cloud".to_string(),
                zone: Some("tk1a".to_string()),
                config: HashMap::new(),
            },
        );

        let mut server_config = HashMap::new();
        server_config.insert("dns_hostname".to_string(), "prod".to_string());

        let mut servers = HashMap::new();
        servers.insert(
            "vps".to_string(),
            ServerResource {
                provider: "sakura-cloud".to_string(),
                config: server_config,
                // aliases なし
                ..Default::default()
            },
        );

        let mut stages = HashMap::new();
        stages.insert(
            "live".to_string(),
            Stage {
                services: vec![],
                servers: vec!["vps".to_string()],
                ..Default::default()
            },
        );

        let flow = fleetflow_core::Flow {
            name: "test".to_string(),
            providers,
            servers,
            stages,
            services: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
        };

        let resource_set = build_resource_set(&flow, "live");
        // server + A レコード のみ
        assert_eq!(resource_set.resources.len(), 2);
        assert!(resource_set.get("dns-record", "dns-a-prod").is_some());
        assert_eq!(resource_set.by_type("dns-record").len(), 1);
    }

    #[test]
    fn test_build_resource_set_empty_servers() {
        let flow = fleetflow_core::Flow {
            name: "empty".to_string(),
            providers: HashMap::new(),
            servers: HashMap::new(),
            stages: HashMap::new(),
            services: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
        };
        let resource_set = build_resource_set(&flow, "dev");
        assert!(resource_set.resources.is_empty());
    }
}
