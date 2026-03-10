//! Control Plane 接続ヘルパー（MCP Server 用）
//!
//! CLI 側の cp_client と同じロジックだが、MCP crate 内で自己完結するよう簡略版として実装。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unison::network::client::ProtocolClient;

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
