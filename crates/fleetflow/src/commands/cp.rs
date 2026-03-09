//! Control Plane リソース管理コマンド
//!
//! fleet tenant / fleet project / fleet server の各サブコマンドを処理する。
//! 実際の CP API 接続は未実装（スタブ）。

use anyhow::{Context, Result};
use colored::Colorize;

use crate::{ProjectCommands, ServerCommands, TenantCommands};

/// 認証済みか確認し、未認証なら案内を出して終了
fn require_login() -> Result<()> {
    let creds_path = dirs::config_dir()
        .context("ホームディレクトリが見つかりません")?
        .join("fleetflow/credentials.json");

    if !creds_path.exists() {
        eprintln!("{}", "Control Plane に未ログインです。".red().bold());
        eprintln!();
        eprintln!("  {} でログインしてください。", "fleet login".cyan());
        std::process::exit(1);
    }

    Ok(())
}

pub async fn handle_tenant(cmd: &TenantCommands) -> Result<()> {
    require_login()?;

    match cmd {
        TenantCommands::Status => {
            println!("{}", "テナント状態".bold());
            println!();
            // TODO: CP API 接続してテナント情報を取得
            println!(
                "{}",
                "Control Plane API への接続は未実装です。".yellow()
            );
            println!("実装予定: Unison Protocol 経由でテナント情報を取得・表示");
        }
    }

    Ok(())
}

pub async fn handle_project(cmd: &ProjectCommands) -> Result<()> {
    require_login()?;

    match cmd {
        ProjectCommands::List => {
            println!("{}", "プロジェクト一覧".bold());
            println!();
            // TODO: CP API 接続
            println!(
                "{}",
                "Control Plane API への接続は未実装です。".yellow()
            );
            println!("実装予定: テナント内の全プロジェクトを一覧表示");
        }
        ProjectCommands::Create { slug, name } => {
            println!("{}", "プロジェクト作成".bold());
            println!("  Slug: {}", slug.cyan());
            println!("  Name: {}", name.cyan());
            println!();
            // TODO: CP API 接続
            println!(
                "{}",
                "Control Plane API への接続は未実装です。".yellow()
            );
        }
        ProjectCommands::Show { slug } => {
            println!("{} {}", "プロジェクト詳細:".bold(), slug.cyan());
            println!();
            // TODO: CP API 接続
            println!(
                "{}",
                "Control Plane API への接続は未実装です。".yellow()
            );
        }
    }

    Ok(())
}

pub async fn handle_server(cmd: &ServerCommands) -> Result<()> {
    require_login()?;

    match cmd {
        ServerCommands::List => {
            println!("{}", "サーバー一覧".bold());
            println!();
            // TODO: CP API 接続
            println!(
                "{}",
                "Control Plane API への接続は未実装です。".yellow()
            );
            println!("実装予定: テナント内の全サーバーを一覧表示");
        }
        ServerCommands::Register {
            slug,
            provider,
            ssh_host,
            deploy_path,
        } => {
            println!("{}", "サーバー登録".bold());
            println!("  Slug:     {}", slug.cyan());
            println!("  Provider: {}", provider.cyan());
            if let Some(host) = ssh_host {
                println!("  SSH Host: {}", host.cyan());
            }
            if let Some(path) = deploy_path {
                println!("  Deploy:   {}", path.cyan());
            }
            println!();
            // TODO: CP API 接続
            println!(
                "{}",
                "Control Plane API への接続は未実装です。".yellow()
            );
        }
        ServerCommands::Status { slug } => {
            println!("{} {}", "サーバー状態:".bold(), slug.cyan());
            println!();
            // TODO: CP API 接続
            println!(
                "{}",
                "Control Plane API への接続は未実装です。".yellow()
            );
        }
    }

    Ok(())
}
