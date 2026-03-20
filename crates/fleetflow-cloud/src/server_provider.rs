//! サーバーライフサイクル管理の抽象化
//!
//! CloudProvider（IaC 宣言型）とは異なり、サーバーの命令型 CRUD 操作を提供する。
//! 各クラウドプロバイダーがこの trait を実装する。

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// サーバーライフサイクル管理の trait
///
/// クラウドプロバイダー上のサーバー実体を操作する。
/// DB メタデータ管理（CP 側）とは分離された、純粋なインフラ操作層。
///
/// Note: async fn in trait は object-safe でないため、
/// ランタイムディスパッチには `ServerProviderKind` enum を使う。
#[allow(async_fn_in_trait)]
pub trait ServerProvider: Send + Sync {
    /// プロバイダー名（e.g., "sakura-cloud"）
    fn provider_name(&self) -> &str;

    /// サーバー一覧取得
    async fn list_servers(&self) -> Result<Vec<ServerSpec>>;

    /// サーバー情報取得（ID指定）
    async fn get_server(&self, server_id: &str) -> Result<ServerSpec>;

    /// サーバー作成
    async fn create_server(&self, request: &CreateServerRequest) -> Result<ServerSpec>;

    /// サーバー削除
    async fn delete_server(&self, server_id: &str, with_disks: bool) -> Result<()>;

    /// 電源 ON
    async fn power_on(&self, server_id: &str) -> Result<()>;

    /// 電源 OFF
    async fn power_off(&self, server_id: &str) -> Result<()>;
}

/// サーバー情報（プロバイダー非依存）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSpec {
    /// プロバイダー上の ID
    pub id: String,

    /// サーバー名
    pub name: String,

    /// CPU コア数
    pub cpu: Option<i32>,

    /// メモリ（GB）
    pub memory_gb: Option<i32>,

    /// ディスクサイズ（GB）
    pub disk_gb: Option<i32>,

    /// 稼働状態
    pub status: ServerStatus,

    /// IP アドレス
    pub ip_address: Option<String>,

    /// プロバイダー名
    pub provider: String,

    /// ゾーン/リージョン
    pub zone: Option<String>,

    /// タグ
    pub tags: Vec<String>,
}

/// サーバー稼働状態
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Running,
    Stopped,
    Unknown,
}

impl std::fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerStatus::Running => write!(f, "running"),
            ServerStatus::Stopped => write!(f, "stopped"),
            ServerStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// サーバー作成リクエスト（プロバイダー非依存）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateServerRequest {
    /// サーバー名
    pub name: String,

    /// CPU コア数
    pub cpu: i32,

    /// メモリ（GB）
    pub memory_gb: i32,

    /// ディスクサイズ（GB）
    pub disk_gb: Option<i32>,

    /// OS タイプ（e.g., "debian", "ubuntu"）
    pub os_type: Option<String>,

    /// SSH 公開鍵名一覧
    pub ssh_keys: Vec<String>,

    /// タグ
    pub tags: Vec<String>,

    /// プロバイダー固有の設定
    pub provider_config: Option<serde_json::Value>,

    /// ネットワーク設定（AWS 等で使用。None の場合はプロバイダーデフォルト）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkConfig>,
}

/// ネットワーク設定（サーバー作成時のネットワーク指定）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Subnet ID
    pub subnet_id: Option<String>,

    /// Security Group ID 一覧
    #[serde(default)]
    pub security_group_ids: Vec<String>,

    /// プライベート IP アドレス（固定指定する場合）
    pub private_ip: Option<String>,

    /// パブリック IP を割り当てるか
    #[serde(default)]
    pub assign_public_ip: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_spec_serialization() {
        let spec = ServerSpec {
            id: "123".into(),
            name: "test-server".into(),
            cpu: Some(2),
            memory_gb: Some(4),
            disk_gb: Some(40),
            status: ServerStatus::Running,
            ip_address: Some("203.0.113.1".into()),
            provider: "sakura-cloud".into(),
            zone: Some("tk1a".into()),
            tags: vec!["fleetflow".into()],
        };

        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: ServerSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "123");
        assert_eq!(deserialized.name, "test-server");
        assert_eq!(deserialized.status, ServerStatus::Running);
    }

    #[test]
    fn test_server_status_display() {
        assert_eq!(ServerStatus::Running.to_string(), "running");
        assert_eq!(ServerStatus::Stopped.to_string(), "stopped");
        assert_eq!(ServerStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_create_server_request_serialization() {
        let request = CreateServerRequest {
            name: "fleet-worker-01".into(),
            cpu: 2,
            memory_gb: 4,
            disk_gb: Some(40),
            os_type: Some("debian".into()),
            ssh_keys: vec!["my-key".into()],
            tags: vec!["fleetflow".into(), "worker".into()],
            provider_config: None,
            network: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateServerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "fleet-worker-01");
        assert_eq!(deserialized.cpu, 2);
        assert_eq!(deserialized.memory_gb, 4);
    }

    #[test]
    fn test_create_server_request_with_network() {
        let request = CreateServerRequest {
            name: "aws-web".into(),
            cpu: 2,
            memory_gb: 4,
            disk_gb: None,
            os_type: Some("ubuntu-24.04".into()),
            ssh_keys: vec!["my-key".into()],
            tags: vec![],
            provider_config: None,
            network: Some(NetworkConfig {
                subnet_id: Some("subnet-abc123".into()),
                security_group_ids: vec!["sg-def456".into()],
                private_ip: None,
                assign_public_ip: false,
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateServerRequest = serde_json::from_str(&json).unwrap();
        let net = deserialized.network.unwrap();
        assert_eq!(net.subnet_id, Some("subnet-abc123".into()));
        assert_eq!(net.security_group_ids, vec!["sg-def456"]);
        assert!(!net.assign_public_ip);
    }

    #[test]
    fn test_create_server_request_network_none_compat() {
        // network フィールドなしの JSON でもデシリアライズ可能（後方互換）
        let json = r#"{"name":"srv","cpu":2,"memory_gb":4,"ssh_keys":[],"tags":[]}"#;
        let request: CreateServerRequest = serde_json::from_str(json).unwrap();
        assert!(request.network.is_none());
    }

    #[test]
    fn test_server_status_serde_roundtrip() {
        for status in [
            ServerStatus::Running,
            ServerStatus::Stopped,
            ServerStatus::Unknown,
        ] {
            let json = serde_json::to_value(&status).unwrap();
            let deserialized: ServerStatus = serde_json::from_value(json).unwrap();
            assert_eq!(deserialized, status);
        }
    }
}
