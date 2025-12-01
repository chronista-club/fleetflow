//! クラウドリソースモデル
//!
//! FleetFlowで管理するクラウドリソース（サーバー、プロバイダーなど）の定義

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// クラウドプロバイダー設定
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloudProvider {
    /// プロバイダー名（sakura-cloud, cloudflare など）
    pub name: String,

    /// ゾーン/リージョン（tk1a, is1b など）
    pub zone: Option<String>,

    /// 追加設定（プロバイダー固有）
    pub config: HashMap<String, String>,
}

/// サーバーリソース設定
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerResource {
    /// 使用するプロバイダー名
    pub provider: String,

    /// サーバープラン（2core-4gb, 4core-8gb など）
    pub plan: Option<String>,

    /// ディスクサイズ (GB)
    pub disk_size: Option<u32>,

    /// SSHキー名
    pub ssh_keys: Vec<String>,

    /// OSイメージ
    pub os: Option<String>,

    /// スタートアップスクリプト
    pub startup_script: Option<String>,

    /// タグ
    pub tags: Vec<String>,

    /// DNSエイリアス（CNAME）の一覧
    /// 例: ["app", "api"] -> app.{domain} と api.{domain} が {server-hostname}.{domain} を参照
    pub dns_aliases: Vec<String>,

    /// 追加設定
    pub config: HashMap<String, String>,
}

impl ServerResource {
    /// デフォルト値でサーバーリソースを作成
    pub fn with_provider(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            ..Default::default()
        }
    }
}
