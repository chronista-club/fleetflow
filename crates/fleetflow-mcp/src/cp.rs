//! Control Plane 接続ヘルパー（MCP Server 用）
//!
//! CLI 側の cp_client と同じロジックだが、MCP crate 内で自己完結するよう簡略版として実装。

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unison::network::client::ProtocolClient;
use unison::network::quic::QuicClient;
use unison::network::trust::TrustAnchors;

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

    let client = build_cp_client().context("Unison ProtocolClient 作成失敗")?;

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

    let resp: Value = channel
        .request(method, &payload)
        .await
        .with_context(|| format!("{}.{} リクエスト失敗", channel_name, method))?;

    channel.close().await.ok();

    if let Some(err) = resp.get("error").and_then(|e| e.as_str()) {
        anyhow::bail!("CP エラー: {}", err);
    }

    Ok(resp)
}

/// CP の CA 証明書ファイルのパス。
///
/// `FLEETFLOW_CP_CA_CERT` env が優先、無ければ
/// `~/.config/fleetflow/cp-ca-cert.pem`（OS 依存）。
fn ca_cert_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("FLEETFLOW_CP_CA_CERT") {
        return Ok(PathBuf::from(p));
    }
    Ok(dirs::config_dir()
        .context("設定ディレクトリが見つかりません")?
        .join("fleetflow/cp-ca-cert.pem"))
}

/// CP の MeshCa CA cert を pin した `ProtocolClient` を構築する。
///
/// CP が配布する公開 CA cert を `TrustAnchors::Custom` に入れる。rustls が
/// CP の server cert を CA 署名 chain として検証する（MITM 耐性）。
fn build_cp_client() -> Result<ProtocolClient> {
    let ca_path = ca_cert_path()?;
    let ca_pem = std::fs::read(&ca_path).with_context(|| {
        format!(
            "CP の CA 証明書が見つかりません: {}\n  \
             CP 管理者から cp-ca-cert.pem を取得し配置してください \
             (または FLEETFLOW_CP_CA_CERT で指定)。",
            ca_path.display()
        )
    })?;
    let mut rd: &[u8] = &ca_pem;
    let certs = rustls_pemfile::certs(&mut rd)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("CA 証明書 PEM のパース失敗")?;
    anyhow::ensure!(!certs.is_empty(), "CA 証明書 PEM に証明書がありません");
    let transport = QuicClient::builder()
        .trust_anchors(TrustAnchors::Custom(certs))
        .build()
        .context("QuicClient の構築失敗")?;
    Ok(ProtocolClient::new(transport))
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
/// MCP サーバーはセッション中ずっと稼働するため、
/// 毎回ファイルを読み直して期限切れを防ぐ。
fn load_access_token() -> Result<Option<String>> {
    let creds_path = match dirs::config_dir() {
        Some(d) => d.join("fleetflow/credentials.json"),
        None => return Ok(None),
    };

    if !creds_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&creds_path).context("credentials 読み込み失敗")?;
    let creds: Credentials = serde_json::from_str(&content).context("credentials パース失敗")?;

    // 有効期限チェック
    if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&creds.expires_at)
        && expires < chrono::Utc::now()
    {
        return Ok(None); // 期限切れ → トークンなしで送信（サーバー側で 401）
    }

    Ok(Some(creds.access_token))
}
