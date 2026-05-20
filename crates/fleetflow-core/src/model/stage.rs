//! ステージ定義

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ステージの実行 backend（コンテナをどのランタイムで動かすか）。
///
/// Podman+Quadlet 追従 epic（creo-memories `mem_1CbD3b6j1s3pxQ1TGvaXtv`）WS2。
/// stage ごとに明示宣言する（stage 名からの暗黙推論はしない — 慣習依存で脆い）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    /// Docker daemon（bollard 経由）— 既定。`runtime` 未宣言時はこれ。
    #[default]
    Docker,
    /// Podman + Quadlet（systemd 管理の `.container`/`.network`）。
    Quadlet,
    /// Compose（`podman compose` / `docker compose`）。
    Compose,
}

impl Runtime {
    /// 文字列からパースする（KDL `runtime "..."` ノード用）。
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
    /// 実行 backend。KDL `runtime "quadlet"` で宣言。未宣言時は `Docker`。
    #[serde(default)]
    pub runtime: Runtime,
}
