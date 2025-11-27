//! サービス定義

use super::port::Port;
use super::volume::Volume;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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
