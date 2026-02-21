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

    /// OSイメージ（archive未指定時に使用）
    pub os: Option<String>,

    /// アーカイブ名またはID（os より優先）
    /// 例: "creo-base-v1" または "113703014367"
    pub archive: Option<String>,

    /// スタートアップスクリプト（note名）
    pub startup_script: Option<String>,

    /// スタートアップスクリプトに渡す変数
    /// 例: {"SSH_PUBKEY": "ssh-rsa ...", "TAILSCALE_AUTHKEY": "tskey-..."}
    pub init_script_vars: HashMap<String, String>,

    /// タグ
    pub tags: Vec<String>,

    /// DNSエイリアス（CNAME）の一覧
    /// 例: ["app", "api"] -> app.{domain} と api.{domain} が {server-hostname}.{domain} を参照
    pub dns_aliases: Vec<String>,

    /// デプロイ先パス
    /// 例: "/opt/myapp" - CI/CDやmiseタスクでデプロイ先を参照
    pub deploy_path: Option<String>,

    /// SSH接続先（IPアドレスまたはホスト名）
    /// 例: "153.xxx.xxx.xxx" - fleet registry deploy でSSH経由のリモートデプロイに使用
    pub ssh_host: Option<String>,

    /// SSHユーザー名（デフォルト: "root"）
    pub ssh_user: Option<String>,

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
