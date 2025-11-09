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

/// FlowConfig - Flow設定のルート（後方互換性のため維持）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowConfig {
    pub stages: HashMap<String, Stage>,
    pub services: HashMap<String, Service>,
}

impl From<Flow> for FlowConfig {
    fn from(flow: Flow) -> Self {
        FlowConfig {
            stages: flow.stages,
            services: flow.services,
        }
    }
}

impl FlowConfig {
    /// FlowConfigからFlowを作成（デフォルト名を使用）
    pub fn into_flow(self, name: String) -> Flow {
        Flow {
            name,
            stages: self.stages,
            services: self.services,
        }
    }
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
