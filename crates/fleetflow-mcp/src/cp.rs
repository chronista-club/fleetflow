//! Control Plane 接続ヘルパー（MCP Server 用）
//!
//! CLI 側の cp_client と同じロジックだが、MCP crate 内で自己完結するよう簡略版として実装。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unison::network::client::ProtocolClient;

/// Dashboard API のベース URL
const DASHBOARD_BASE: &str = "http://127.0.0.1:32080";

/// credentials.json の構造
#[derive(Serialize, Deserialize, Debug)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: String,
    pub api_endpoint: String,
    pub tenant_slug: Option<String>,
    pub email: Option<String>,
}

/// CP サーバーへの接続を確立
pub async fn connect() -> Result<(ProtocolClient, Credentials)> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let creds_path = dirs::config_dir()
        .context("ホームディレクトリが見つかりません")?
        .join("fleetflow/credentials.json");

    if !creds_path.exists() {
        anyhow::bail!("CP に未ログイン。`fleet login` でログインしてください。");
    }

    let content =
        std::fs::read_to_string(&creds_path).context("credentials.json の読み込み失敗")?;
    let creds: Credentials =
        serde_json::from_str(&content).context("credentials.json のパース失敗")?;

    // 有効期限チェック
    if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&creds.expires_at)
        && expires < chrono::Utc::now()
    {
        anyhow::bail!("認証トークンの有効期限切れ。`fleet login` で再認証してください。");
    }

    let client = ProtocolClient::new_default().context("Unison ProtocolClient 作成失敗")?;

    client
        .connect(&creds.api_endpoint)
        .await
        .with_context(|| format!("CP サーバーへの接続失敗: {}", creds.api_endpoint))?;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    Ok((client, creds))
}

/// チャネルを開いてリクエストを送信
pub async fn request(
    client: &ProtocolClient,
    channel_name: &str,
    method: &str,
    payload: Value,
) -> Result<Value> {
    let channel = client
        .open_channel(channel_name)
        .await
        .with_context(|| format!("チャネル '{}' オープン失敗", channel_name))?;

    let resp = channel
        .request(method, payload)
        .await
        .with_context(|| format!("{}.{} リクエスト失敗", channel_name, method))?;

    channel.close().await.ok();

    if let Some(err) = resp.get("error").and_then(|e| e.as_str()) {
        anyhow::bail!("CP エラー: {}", err);
    }

    Ok(resp)
}

/// 共有 HTTP クライアント（コネクションプール再利用）
fn http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// Dashboard HTTP API に GET リクエスト
pub async fn http_get(path: &str) -> Result<Value> {
    let url = format!("{}{}", DASHBOARD_BASE, path);
    let token = load_access_token()?;

    let mut req = http_client().get(&url);
    if let Some(t) = &token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }

    let resp = req.send().await.context("HTTP GET 失敗")?;
    let status = resp.status();
    let body: Value = resp.json().await.context("JSON パース失敗")?;

    if !status.is_success() {
        let err = body["error"].as_str().unwrap_or("unknown error");
        anyhow::bail!("HTTP {} — {}", status, err);
    }

    Ok(body)
}

/// Dashboard HTTP API に POST リクエスト（オプションでボディ付き）
pub async fn http_post(path: &str) -> Result<Value> {
    http_post_with_body(path, None).await
}

/// Dashboard HTTP API に POST リクエスト（ボディ付き）
pub async fn http_post_with_body(path: &str, body: Option<Value>) -> Result<Value> {
    let url = format!("{}{}", DASHBOARD_BASE, path);
    let token = load_access_token()?;

    let mut req = http_client().post(&url);
    if let Some(t) = &token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }
    if let Some(b) = body {
        req = req.json(&b); // reqwest が Content-Type: application/json を自動設定
    }

    let resp = req.send().await.context("HTTP POST 失敗")?;
    let status = resp.status();
    let resp_body: Value = resp.json().await.context("JSON パース失敗")?;

    if !status.is_success() {
        let err = resp_body["error"].as_str().unwrap_or("unknown error");
        anyhow::bail!("HTTP {} — {}", status, err);
    }

    Ok(resp_body)
}

/// credentials.json から access_token を取得
fn load_access_token() -> Result<Option<String>> {
    let creds_path = dirs::config_dir()
        .context("設定ディレクトリなし")?
        .join("fleetflow/credentials.json");

    if !creds_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&creds_path)?;
    let creds: Credentials = serde_json::from_str(&content)?;
    Ok(Some(creds.access_token))
}
