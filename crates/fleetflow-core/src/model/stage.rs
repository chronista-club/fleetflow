//! ステージ定義

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ステージの実行 backend（fleetflow がどの方式でコンテナを動かすか）。
///
/// Podman+Quadlet 追従 epic（creo-memories `mem_1CbD3b6j1s3pxQ1TGvaXtv`）WS2。
/// stage ごとに明示宣言する（stage 名からの暗黙推論はしない — 慣習依存で脆い）。
///
/// 注: これは「コンテナエンジン」そのものではなく fleetflow の駆動方式。
/// `Quadlet` のエンジンは Podman（Quadlet は Docker には無い systemd 統合）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// Docker daemon を bollard API で駆動 — 既定。`backend` 未宣言時はこれ。
    #[default]
    Docker,
    /// Podman + Quadlet（systemd 管理の `.container`/`.network`）。
    Quadlet,
    /// Compose（`podman compose` / `docker compose`）。
    Compose,
}

impl Backend {
    /// 文字列からパースする（KDL `backend "..."` ノード用）。
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "docker" => Some(Self::Docker),
            "quadlet" => Some(Self::Quadlet),
            "compose" => Some(Self::Compose),
            _ => None,
        }
    }

    /// 文字列表現。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Quadlet => "quadlet",
            Self::Compose => "compose",
        }
    }
}

/// ステージ定義
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stage {
    /// このステージで起動するサービスのリスト
    #[serde(default)]
    pub services: Vec<String>,
    /// このステージで必要なサーバーのリスト
    #[serde(default)]
    pub servers: Vec<String>,
    /// ステージ固有の環境変数
    #[serde(default)]
    pub variables: HashMap<String, String>,
    /// ステージ固有のコンテナレジストリURL（例: ghcr.io/owner）
    #[serde(default)]
    pub registry: Option<String>,
    /// 実行 backend。KDL `backend "quadlet"` で宣言。未宣言時は `Docker`。
    #[serde(default)]
    pub backend: Backend,
}
