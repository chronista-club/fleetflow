//! Control Plane クライアント — Unison Protocol 経由で CP サーバーに接続
//!
//! credentials.json から接続先とトークンを読み込み、
//! ProtocolClient で CP に接続して channel を開く。

use std::path::PathBuf;

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::Value;
use unison::network::client::ProtocolClient;
use unison::network::quic::QuicClient;
use unison::network::trust::TrustAnchors;

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
        eprintln!("  {} でログインしてください。", "fleet cp login".cyan());
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
        eprintln!("  {} で再認証してください。", "fleet cp login".cyan());
        std::process::exit(1);
    }

    let client = build_cp_client().context("Unison ProtocolClient 作成失敗")?;

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

    let resp: Value = channel
        .request(method, &payload)
        .await
        .with_context(|| format!("{}.{} リクエスト失敗", channel_name, method))?;

    channel.close().await.ok();

    // エラーチェック
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
