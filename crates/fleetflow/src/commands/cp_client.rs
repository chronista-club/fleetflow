//! Control Plane クライアント — Unison Protocol 経由で CP サーバーに接続
//!
//! credentials.json から接続先とトークンを読み込み、
//! ProtocolClient で CP に接続して channel を開く。

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::Value;
use unison::network::client::ProtocolClient;

use super::auth::Credentials;

/// CP サーバーへの接続を確立
pub async fn connect() -> Result<(ProtocolClient, Credentials)> {
    // Rustls CryptoProvider（Unison/QUIC に必要）
    let _ = rustls::crypto::ring::default_provider().install_default();

    let creds_path = dirs::config_dir()
        .context("ホームディレクトリが見つかりません")?
        .join("fleetflow/credentials.json");

    if !creds_path.exists() {
        eprintln!("{}", "Control Plane に未ログインです。".red().bold());
        eprintln!();
        eprintln!("  {} でログインしてください。", "fleet login".cyan());
        std::process::exit(1);
    }

    let content =
        std::fs::read_to_string(&creds_path).context("credentials.json の読み込み失敗")?;
    let creds: Credentials =
        serde_json::from_str(&content).context("credentials.json のパース失敗")?;

    // 有効期限チェック
    if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&creds.expires_at)
        && expires < chrono::Utc::now()
    {
        eprintln!("{}", "認証トークンの有効期限が切れています。".red().bold());
        eprintln!();
        eprintln!("  {} で再認証してください。", "fleet login".cyan());
        std::process::exit(1);
    }

    let client = ProtocolClient::new_default().context("Unison ProtocolClient 作成失敗")?;

    client
        .connect(&creds.api_endpoint)
        .await
        .with_context(|| format!("CP サーバーへの接続失敗: {}", creds.api_endpoint))?;

    // Identity handshake を待機
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    Ok((client, creds))
}

/// チャネルを開いてリクエストを送信し、レスポンスを返す
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

    // エラーチェック
    if let Some(err) = resp.get("error").and_then(|e| e.as_str()) {
        anyhow::bail!("CP エラー: {}", err);
    }

    Ok(resp)
}
