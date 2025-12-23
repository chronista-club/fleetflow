//! ステージ定義

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}
