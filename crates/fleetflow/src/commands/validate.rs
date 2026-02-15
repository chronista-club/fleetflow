use colored::Colorize;

pub async fn handle() -> anyhow::Result<()> {
    println!("{}", "設定を検証中...".blue());

    // プロジェクトルートを検出
    match fleetflow_core::find_project_root() {
        Ok(project_root) => {
            println!(
                "プロジェクトルート: {}",
                project_root.display().to_string().cyan()
            );

            // デバッグモードでロード
            match fleetflow_core::load_project_with_debug(&project_root) {
                Ok(config) => {
                    println!("{}", "✓ 設定ファイルは正常です！".green().bold());
                    println!();
                    println!("サマリー:");
                    println!("  サービス: {}個", config.services.len());
                    for (name, service) in &config.services {
                        let image = service
                            .image
                            .as_ref()
                            .or(service.version.as_ref())
                            .map(|s| s.as_str())
                            .unwrap_or("(未設定)");
                        println!("    - {} ({})", name.cyan(), image);
                    }
                    println!("  ステージ: {}個", config.stages.len());
                    for (name, stage) in &config.stages {
                        let server_info = if stage.servers.is_empty() {
                            String::new()
                        } else {
                            format!(", {}個のサーバー", stage.servers.len())
                        };
                        println!(
                            "    - {} ({}個のサービス{})",
                            name.cyan(),
                            stage.services.len(),
                            server_info
                        );
                    }

                    // クラウドリソースの表示
                    if !config.providers.is_empty() {
                        println!("  プロバイダー: {}個", config.providers.len());
                        for (name, provider) in &config.providers {
                            let zone = provider.zone.as_deref().unwrap_or("(未設定)");
                            println!("    - {} (zone: {})", name.cyan(), zone);
                        }
                    }
                    if !config.servers.is_empty() {
                        println!("  サーバー: {}個", config.servers.len());
                        for (name, server) in &config.servers {
                            println!("    - {} ({})", name.cyan(), server.provider);
                        }
                    }
                }
                Err(e) => {
                    eprintln!();
                    eprintln!("{}", "✗ 設定エラー".red().bold());
                    eprintln!("  {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!();
            eprintln!("{}", "✗ プロジェクトルートが見つかりません".red().bold());
            eprintln!("  {}", e);
            eprintln!();
            eprintln!("fleet.kdl が存在するディレクトリで実行してください");
            std::process::exit(1);
        }
    }

    Ok(())
}
