//! Tailscale ステータス取得・ヘルスチェック
//!
//! `tailscale status --json` と `tailscale ping` をパースして
//! FleetFlow のサーバーステータス管理に利用する。

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::CloudError;

/// `tailscale status --json` のトップレベル構造
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TailscaleStatus {
    /// 自ノード情報
    #[serde(rename = "Self")]
    pub self_node: TailscaleNode,
    /// ピアノード（NodeKey → Node）
    pub peer: HashMap<String, TailscaleNode>,
}

/// Tailscale ノード情報
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TailscaleNode {
    /// ホスト名
    #[serde(rename = "HostName")]
    pub hostname: String,
    /// DNS 名（FQDN）
    #[serde(rename = "DNSName")]
    pub dns_name: String,
    /// Tailscale IP アドレス
    #[serde(rename = "TailscaleIPs")]
    pub tailscale_ips: Vec<String>,
    /// オンライン状態
    pub online: bool,
    /// OS 種別
    #[serde(rename = "OS")]
    pub os: String,
    /// 最終確認時刻
    pub last_seen: DateTime<Utc>,
}

/// ping 結果
#[derive(Debug, Clone, Serialize)]
pub struct PingResult {
    pub hostname: String,
    pub reachable: bool,
    pub latency_ms: Option<f64>,
    pub via: Option<String>,
}

/// `tailscale status --json` を実行してパース
pub async fn get_status() -> Result<TailscaleStatus, CloudError> {
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await
        .map_err(|e| CloudError::CommandFailed(format!("tailscale コマンド実行失敗: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CloudError::CommandFailed(format!(
            "tailscale status 失敗: {stderr}"
        )));
    }

    let status: TailscaleStatus = serde_json::from_slice(&output.stdout)
        .map_err(|e| CloudError::CommandFailed(format!("tailscale JSON パース失敗: {e}")))?;

    Ok(status)
}

/// 全ピアをフラットなリストとして取得
pub async fn get_peers() -> Result<Vec<TailscaleNode>, CloudError> {
    let status = get_status().await?;
    Ok(status.peer.into_values().collect())
}

/// ホスト名でピアを検索
pub async fn find_peer(hostname: &str) -> Result<Option<TailscaleNode>, CloudError> {
    let status = get_status().await?;
    let node = status
        .peer
        .into_values()
        .find(|n| n.hostname.eq_ignore_ascii_case(hostname));
    Ok(node)
}

/// `tailscale ping` を実行して疎通確認
pub async fn ping(hostname: &str) -> Result<PingResult, CloudError> {
    let output = Command::new("tailscale")
        .args(["ping", "--c", "1", "--timeout", "5s", hostname])
        .output()
        .await
        .map_err(|e| CloudError::CommandFailed(format!("tailscale ping 実行失敗: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() {
        return Ok(PingResult {
            hostname: hostname.to_string(),
            reachable: false,
            latency_ms: None,
            via: None,
        });
    }

    // 出力例: "pong from creo-prod (100.80.54.111) via DERP(tok) in 32ms"
    //       "pong from creo-prod (100.80.54.111) via 203.0.113.1:41641 in 5ms"
    let (latency_ms, via) = parse_pong_line(&stdout);

    Ok(PingResult {
        hostname: hostname.to_string(),
        reachable: true,
        latency_ms,
        via,
    })
}

/// pong 出力行からレイテンシと経路を抽出
fn parse_pong_line(output: &str) -> (Option<f64>, Option<String>) {
    let line = output.lines().find(|l| l.starts_with("pong from"));
    let Some(line) = line else {
        return (None, None);
    };

    // "via XXXX" を抽出
    let via = line.find("via ").map(|i| {
        let rest = &line[i + 4..];
        rest.split(" in ").next().unwrap_or(rest).to_string()
    });

    // "in XXms" を抽出
    let latency_ms = line.rfind(" in ").and_then(|i| {
        let rest = &line[i + 4..];
        rest.trim_end_matches("ms").trim().parse::<f64>().ok()
    });

    (latency_ms, via)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pong_derp() {
        let output = "pong from creo-prod (100.80.54.111) via DERP(tok) in 32ms\n";
        let (latency, via) = parse_pong_line(output);
        assert_eq!(latency, Some(32.0));
        assert_eq!(via.as_deref(), Some("DERP(tok)"));
    }

    #[test]
    fn test_parse_pong_direct() {
        let output = "pong from creo-dev (100.79.253.84) via 203.0.113.1:41641 in 5ms\n";
        let (latency, via) = parse_pong_line(output);
        assert_eq!(latency, Some(5.0));
        assert_eq!(via.as_deref(), Some("203.0.113.1:41641"));
    }

    #[test]
    fn test_parse_pong_no_match() {
        let output = "timeout waiting for pong\n";
        let (latency, via) = parse_pong_line(output);
        assert!(latency.is_none());
        assert!(via.is_none());
    }

    #[test]
    fn test_deserialize_tailscale_node() {
        let json = r#"{
            "HostName": "creo-prod",
            "DNSName": "creo-prod.tail12345.ts.net.",
            "TailscaleIPs": ["100.80.54.111"],
            "Online": true,
            "OS": "linux",
            "LastSeen": "2026-03-09T10:00:00Z"
        }"#;
        let node: TailscaleNode = serde_json::from_str(json).unwrap();
        assert_eq!(node.hostname, "creo-prod");
        assert!(node.online);
        assert_eq!(node.tailscale_ips, vec!["100.80.54.111"]);
        assert_eq!(node.os, "linux");
    }

    #[test]
    fn test_deserialize_tailscale_status() {
        let json = r#"{
            "Self": {
                "HostName": "makoto-mac",
                "DNSName": "makoto-mac.tail12345.ts.net.",
                "TailscaleIPs": ["100.100.1.1"],
                "Online": true,
                "OS": "macOS",
                "LastSeen": "2026-03-10T00:00:00Z"
            },
            "Peer": {
                "nodekey:abc123": {
                    "HostName": "creo-prod",
                    "DNSName": "creo-prod.tail12345.ts.net.",
                    "TailscaleIPs": ["100.80.54.111"],
                    "Online": true,
                    "OS": "linux",
                    "LastSeen": "2026-03-09T10:00:00Z"
                },
                "nodekey:def456": {
                    "HostName": "creo-dev",
                    "DNSName": "creo-dev.tail12345.ts.net.",
                    "TailscaleIPs": ["100.79.253.84"],
                    "Online": false,
                    "OS": "linux",
                    "LastSeen": "2025-12-24T16:50:20Z"
                }
            }
        }"#;
        let status: TailscaleStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.self_node.hostname, "makoto-mac");
        assert_eq!(status.peer.len(), 2);

        let prod = status
            .peer
            .values()
            .find(|n| n.hostname == "creo-prod")
            .unwrap();
        assert!(prod.online);

        let dev = status
            .peer
            .values()
            .find(|n| n.hostname == "creo-dev")
            .unwrap();
        assert!(!dev.online);
    }
}
