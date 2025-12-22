//! プロセス定義

use super::port::Port;
use serde::{Deserialize, Serialize};

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
