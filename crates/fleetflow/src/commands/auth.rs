use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Auth0 のデフォルト設定 (Creo ID / FleetStage Control Plane)
///
/// OSS 利用者は下記 env var で上書き可能:
///   - `FLEETFLOW_AUTH0_DOMAIN`
///   - `FLEETFLOW_AUTH0_CLIENT_ID`
///   - `FLEETFLOW_AUTH0_AUDIENCE`
///   - `FLEETFLOW_CP_ENDPOINT`
const AUTH0_DOMAIN: &str = "anycreative.jp.auth0.com";
const AUTH0_CLIENT_ID: &str = "u3pDPrEoMl5lb9kSa0qHU9g8cDDN9I7N";
const AUTH0_AUDIENCE: &str = "https://api.fleetstage.cloud";
const DEFAULT_CP_ENDPOINT: &str = "https://cp.fleetstage.cloud:4510";

/// Credentials file path: ~/.config/fleetflow/credentials.json
fn credentials_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("ホームディレクトリが見つかりません")?
        .join("fleetflow");
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir.join("credentials.json"))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: String,
    pub api_endpoint: String,
    pub tenant_slug: Option<String>,
    pub email: Option<String>,
}

/// Auth0 Device Authorization Response
#[derive(Deserialize, Debug)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri_complete: String,
    #[allow(dead_code)]
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// Auth0 Token Response
#[derive(Deserialize, Debug)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

/// Auth0 Token Error Response
#[derive(Deserialize, Debug)]
struct TokenErrorResponse {
    error: String,
    #[allow(dead_code)]
    error_description: Option<String>,
}

/// `fleet cp login` — Auth0 Device Authorization Flow
pub async fn handle_login(api_endpoint: Option<String>) -> Result<()> {
    let endpoint = api_endpoint
        .or_else(|| std::env::var("FLEETFLOW_CP_ENDPOINT").ok())
        .unwrap_or_else(|| DEFAULT_CP_ENDPOINT.into());
    let auth0_domain =
        std::env::var("FLEETFLOW_AUTH0_DOMAIN").unwrap_or_else(|_| AUTH0_DOMAIN.into());
    let client_id =
        std::env::var("FLEETFLOW_AUTH0_CLIENT_ID").unwrap_or_else(|_| AUTH0_CLIENT_ID.into());
    let audience =
        std::env::var("FLEETFLOW_AUTH0_AUDIENCE").unwrap_or_else(|_| AUTH0_AUDIENCE.into());

    println!("{}", "FleetFlow Control Plane ログイン".bold());
    println!();

    let http = reqwest::Client::new();

    // Step 1: Request device code
    let device_code_url = format!("https://{}/oauth/device/code", auth0_domain);
    let resp = http
        .post(&device_code_url)
        .form(&[
            ("client_id", client_id.as_str()),
            ("scope", "openid profile email offline_access"),
            ("audience", audience.as_str()),
        ])
        .send()
        .await
        .context("Auth0 Device Code リクエスト失敗")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Auth0 Device Code リクエスト失敗 ({}): {}", status, body);
    }

    let device: DeviceCodeResponse = resp
        .json()
        .await
        .context("Device Code レスポンスのパース失敗")?;

    // Step 2: Show user code and open browser
    println!("ブラウザで以下のURLを開いてコードを入力してください:");
    println!();
    println!(
        "  URL:  {}",
        device.verification_uri_complete.cyan().underline()
    );
    println!("  Code: {}", device.user_code.bold().green());
    println!();

    // Try to open browser automatically
    if open::that(&device.verification_uri_complete).is_ok() {
        println!("{}", "ブラウザを開きました。".dimmed());
    } else {
        println!(
            "{}",
            "ブラウザを自動的に開けませんでした。手動で上記URLを開いてください。".yellow()
        );
    }

    println!();
    println!(
        "認証を待機中... ({} 秒以内に完了してください)",
        device.expires_in
    );

    // Step 3: Poll for token
    let token_url = format!("https://{}/oauth/token", auth0_domain);
    let poll_interval = std::time::Duration::from_secs(device.interval.max(5));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(device.expires_in);

    let token = loop {
        if std::time::Instant::now() > deadline {
            anyhow::bail!("認証がタイムアウトしました。再度 `fleet cp login` を実行してください。");
        }

        tokio::time::sleep(poll_interval).await;

        let resp = http
            .post(&token_url)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &device.device_code),
                ("client_id", &client_id),
            ])
            .send()
            .await
            .context("Token ポーリング失敗")?;

        if resp.status().is_success() {
            let token: TokenResponse = resp.json().await.context("Token レスポンスのパース失敗")?;
            break token;
        }

        // Check error type
        let body = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<TokenErrorResponse>(&body) {
            match err.error.as_str() {
                "authorization_pending" => {
                    // User hasn't authorized yet, continue polling
                    print!(".");
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                    continue;
                }
                "slow_down" => {
                    // Rate limited, increase interval
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
                "expired_token" => {
                    anyhow::bail!(
                        "デバイスコードの有効期限が切れました。再度 `fleet cp login` を実行してください。"
                    );
                }
                "access_denied" => {
                    anyhow::bail!("認証が拒否されました。");
                }
                _ => {
                    anyhow::bail!("Auth0 エラー: {} - {:?}", err.error, err.error_description);
                }
            }
        }
    };

    println!();
    println!();

    // Step 4: Decode JWT to extract email (optional)
    let email = extract_email_from_jwt(&token.access_token);

    // Step 5: Calculate expiration
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(token.expires_in as i64);

    // Step 6: Save credentials
    let creds = Credentials {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: expires_at.to_rfc3339(),
        api_endpoint: endpoint,
        tenant_slug: None, // TODO: テナント情報を API から取得
        email,
    };

    let path = credentials_path()?;
    let json = serde_json::to_string_pretty(&creds)?;
    std::fs::write(&path, &json).context("credentials.json の書き込み失敗")?;

    // Set restrictive permissions (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

    println!("{}", "ログイン成功！".green().bold());
    if let Some(email) = &creds.email {
        println!("  Email: {}", email.cyan());
    }
    println!("  API:   {}", creds.api_endpoint.cyan());

    Ok(())
}

/// JWT から email クレームを簡易抽出（署名検証なし、表示用）
fn extract_email_from_jwt(token: &str) -> Option<String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    use base64::Engine;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;

    let claims: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    claims["email"].as_str().map(String::from)
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
        println!("  `fleet cp login` でログインしてください。");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path).context("credentials.json の読み込みに失敗")?;
    let creds: Credentials =
        serde_json::from_str(&content).context("credentials.json のパースに失敗")?;

    // 有効期限チェック
    let expired = if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&creds.expires_at) {
        expires < chrono::Utc::now()
    } else {
        false
    };

    if expired {
        println!("{}", "認証期限切れ".yellow().bold());
        println!("  `fleet cp login` で再認証してください。");
    } else {
        println!("{}", "認証済み".green().bold());
    }

    if let Some(email) = &creds.email {
        println!("  Email:    {}", email.cyan());
    }
    if let Some(tenant) = &creds.tenant_slug {
        println!("  Tenant:   {}", tenant.cyan());
    }
    println!("  API:      {}", creds.api_endpoint.cyan());
    println!("  Expires:  {}", creds.expires_at);

    Ok(())
}
