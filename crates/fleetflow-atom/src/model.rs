use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Flow - プロセスの設計図
///
/// Flowは複数のサービスとステージを定義し、
/// それらがどのように起動・管理されるかを記述します。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    /// Flow名（プロジェクト名）
    pub name: String,
    /// このFlowで定義されるサービス
    pub services: HashMap<String, Service>,
    /// このFlowで定義されるステージ
    pub stages: HashMap<String, Stage>,
}

/// ステージ定義
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stage {
    /// このステージで起動するサービスのリスト
    #[serde(default)]
    pub services: Vec<String>,
    /// ステージ固有の環境変数
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

/// サービス定義
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Service {
    pub image: Option<String>,
    pub version: Option<String>,
    pub command: Option<String>,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    pub volumes: Vec<Volume>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// ビルド設定
    pub build: Option<BuildConfig>,
}

/// ビルド設定
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Dockerfileのパス（プロジェクトルートからの相対パス）
    pub dockerfile: Option<PathBuf>,
    /// ビルドコンテキストのパス（プロジェクトルートからの相対パス）
    /// 未指定の場合はプロジェクトルート
    pub context: Option<PathBuf>,
    /// ビルド引数
    #[serde(default)]
    pub args: HashMap<String, String>,
    /// マルチステージビルドのターゲット
    pub target: Option<String>,
    /// キャッシュ無効化フラグ
    #[serde(default)]
    pub no_cache: bool,
    /// イメージタグの明示的指定
    pub image_tag: Option<String>,
}

/// ポート定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub host: u16,
    pub container: u16,
    #[serde(default = "default_protocol")]
    pub protocol: Protocol,
    pub host_ip: Option<String>,
}

/// プロトコル種別
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Tcp,
    Udp,
}

fn default_protocol() -> Protocol {
    Protocol::Tcp
}

/// ボリューム定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub host: PathBuf,
    pub container: PathBuf,
    #[serde(default)]
    pub read_only: bool,
}

/// Process - 実行中のプロセス情報
///
/// Flowから起動された実際のプロセス（コンテナ）の状態を表します。
/// DBに格納され、実行中のプロセスを追跡・管理します。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Process {
    /// プロセスID（UUID）
    pub id: String,
    /// 関連するFlow名
    pub flow_name: String,
    /// 関連するステージ名
    pub stage_name: String,
    /// サービス名
    pub service_name: String,
    /// コンテナID（Docker/Podman）
    pub container_id: Option<String>,
    /// プロセスID（OS）
    pub pid: Option<u32>,
    /// プロセス状態
    pub state: ProcessState,
    /// 起動時刻（Unix timestamp）
    pub started_at: i64,
    /// 停止時刻（Unix timestamp、停止していない場合はNone）
    pub stopped_at: Option<i64>,
    /// イメージ名
    pub image: String,
    /// 使用メモリ（バイト）
    pub memory_usage: Option<u64>,
    /// CPU使用率（パーセント）
    pub cpu_usage: Option<f64>,
    /// ポートマッピング
    pub ports: Vec<Port>,
}

/// プロセス状態
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProcessState {
    /// 起動中
    Starting,
    /// 実行中
    Running,
    /// 停止中
    Stopping,
    /// 停止済み
    Stopped,
    /// 一時停止
    Paused,
    /// 異常終了
    Failed,
    /// 再起動中
    Restarting,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flow_creation() {
        let mut services = HashMap::new();
        services.insert(
            "api".to_string(),
            Service {
                image: Some("myapp:1.0.0".to_string()),
                ..Default::default()
            },
        );

        let mut stages = HashMap::new();
        stages.insert(
            "local".to_string(),
            Stage {
                services: vec!["api".to_string()],
                variables: HashMap::new(),
            },
        );

        let flow = Flow {
            name: "my-project".to_string(),
            services,
            stages,
        };

        assert_eq!(flow.name, "my-project");
        assert_eq!(flow.services.len(), 1);
        assert_eq!(flow.stages.len(), 1);
        assert!(flow.services.contains_key("api"));
        assert!(flow.stages.contains_key("local"));
    }

    #[test]
    fn test_flow_to_flowconfig_conversion() {
        let mut services = HashMap::new();
        services.insert("db".to_string(), Service::default());

        let mut stages = HashMap::new();
        stages.insert("dev".to_string(), Stage::default());

        let flow = Flow {
            name: "test-flow".to_string(),
            services: services.clone(),
            stages: stages.clone(),
        };

        assert_eq!(flow.services.len(), 1);
        assert_eq!(flow.stages.len(), 1);
        assert!(flow.services.contains_key("db"));
        assert!(flow.stages.contains_key("dev"));
    }

    #[test]
    fn test_process_creation() {
        let process = Process {
            id: "proc-123".to_string(),
            flow_name: "my-flow".to_string(),
            stage_name: "local".to_string(),
            service_name: "api".to_string(),
            container_id: Some("container-abc".to_string()),
            pid: Some(1234),
            state: ProcessState::Running,
            started_at: 1704067200,
            stopped_at: None,
            image: "myapp:1.0.0".to_string(),
            memory_usage: Some(256_000_000),
            cpu_usage: Some(5.5),
            ports: vec![],
        };

        assert_eq!(process.id, "proc-123");
        assert_eq!(process.flow_name, "my-flow");
        assert_eq!(process.state, ProcessState::Running);
        assert_eq!(process.pid, Some(1234));
        assert!(process.stopped_at.is_none());
    }

    #[test]
    fn test_process_state_transitions() {
        let states = vec![
            ProcessState::Starting,
            ProcessState::Running,
            ProcessState::Stopping,
            ProcessState::Stopped,
            ProcessState::Paused,
            ProcessState::Failed,
            ProcessState::Restarting,
        ];

        for state in states {
            let process = Process {
                id: "test".to_string(),
                flow_name: "test".to_string(),
                stage_name: "test".to_string(),
                service_name: "test".to_string(),
                container_id: None,
                pid: None,
                state: state.clone(),
                started_at: 0,
                stopped_at: None,
                image: "test".to_string(),
                memory_usage: None,
                cpu_usage: None,
                ports: vec![],
            };

            assert_eq!(process.state, state);
        }
    }

    #[test]
    fn test_process_with_resource_usage() {
        let process = Process {
            id: "proc-456".to_string(),
            flow_name: "resource-test".to_string(),
            stage_name: "local".to_string(),
            service_name: "db".to_string(),
            container_id: Some("container-xyz".to_string()),
            pid: Some(5678),
            state: ProcessState::Running,
            started_at: 1704067200,
            stopped_at: None,
            image: "postgres:16".to_string(),
            memory_usage: Some(512_000_000), // 512MB
            cpu_usage: Some(10.5),           // 10.5%
            ports: vec![Port {
                host: 5432,
                container: 5432,
                protocol: Protocol::Tcp,
                host_ip: None,
            }],
        };

        assert_eq!(process.memory_usage, Some(512_000_000));
        assert_eq!(process.cpu_usage, Some(10.5));
        assert_eq!(process.ports.len(), 1);
        assert_eq!(process.ports[0].host, 5432);
    }

    #[test]
    fn test_process_serialization() {
        let process = Process {
            id: "proc-789".to_string(),
            flow_name: "serialize-test".to_string(),
            stage_name: "local".to_string(),
            service_name: "api".to_string(),
            container_id: Some("container-123".to_string()),
            pid: Some(9999),
            state: ProcessState::Running,
            started_at: 1704067200,
            stopped_at: None,
            image: "myapp:latest".to_string(),
            memory_usage: Some(128_000_000),
            cpu_usage: Some(2.5),
            ports: vec![],
        };

        // JSON シリアライズ
        let json = serde_json::to_string(&process).unwrap();
        assert!(json.contains("proc-789"));
        assert!(json.contains("serialize-test"));

        // JSON デシリアライズ
        let deserialized: Process = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, process.id);
        assert_eq!(deserialized.flow_name, process.flow_name);
        assert_eq!(deserialized.state, process.state);
    }
}
