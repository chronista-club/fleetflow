use anyhow::{Context, Result};
use colored::Colorize;
use std::path::PathBuf;

/// Credentials file path: ~/.config/fleetflow/credentials.json
fn credentials_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("ホームディレクトリが見つかりません")?
        .join("fleetflow");
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir.join("credentials.json"))
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: String,
    pub api_endpoint: String,
    pub tenant_slug: Option<String>,
    pub email: Option<String>,
}

/// `fleet login` — Auth0 Device Authorization Flow
pub async fn handle_login(api_endpoint: Option<String>) -> Result<()> {
    let endpoint = api_endpoint.unwrap_or_else(|| "https://api.fleetflow.dev:4510".into());

    println!("{}", "FleetFlow Control Plane ログイン".bold());
    println!();

    // TODO: Auth0 Device Authorization Flow 実装
    // 1. POST /oauth/device/code で device_code, user_code, verification_uri を取得
    // 2. ユーザーにブラウザで verification_uri を開くよう案内
    // 3. ポーリングで access_token を取得
    // 4. credentials.json に保存

    println!(
        "{}",
        "Auth0 Device Authorization Flow は未実装です。".yellow()
    );
    println!(
        "API エンドポイント: {}",
        endpoint.cyan()
    );
    println!();
    println!(
        "実装予定: Auth0 の Device Authorization Flow で認証し、",
    );
    println!("トークンを ~/.config/fleetflow/credentials.json に保存します。");

    Ok(())
}

/// `fleet logout` — トークン破棄
pub async fn handle_logout() -> Result<()> {
    let path = credentials_path()?;

    if path.exists() {
        std::fs::remove_file(&path).context("credentials.json の削除に失敗")?;
        println!("{}", "ログアウトしました。".green());
    } else {
        println!("{}", "ログインしていません。".yellow());
    }

    Ok(())
}

/// `fleet auth status` — 認証状態確認
pub async fn handle_auth_status() -> Result<()> {
    let path = credentials_path()?;

    if !path.exists() {
        println!("{}", "未認証".red().bold());
        println!("  `fleet login` でログインしてください。");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path).context("credentials.json の読み込みに失敗")?;
    let creds: Credentials =
        serde_json::from_str(&content).context("credentials.json のパースに失敗")?;

    println!("{}", "認証済み".green().bold());
    if let Some(email) = &creds.email {
        println!("  Email:    {}", email.cyan());
    }
    if let Some(tenant) = &creds.tenant_slug {
        println!("  Tenant:   {}", tenant.cyan());
    }
    println!("  API:      {}", creds.api_endpoint.cyan());
    println!("  Expires:  {}", creds.expires_at);

    // TODO: トークンの有効期限チェック
    // TODO: API に接続してテナント情報を取得

    Ok(())
}
