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
    /// ヘルスチェック設定
    pub healthcheck: Option<HealthCheck>,
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

/// ヘルスチェック設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// テストコマンド (CMD-SHELL形式またはCMD形式)
    pub test: Vec<String>,
    /// チェック間隔（秒）
    #[serde(default = "default_interval")]
    pub interval: u64,
    /// タイムアウト（秒）
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// リトライ回数
    #[serde(default = "default_retries")]
    pub retries: u64,
    /// 起動待機時間（秒）
    #[serde(default = "default_start_period")]
    pub start_period: u64,
}

fn default_interval() -> u64 {
    30
}
fn default_timeout() -> u64 {
    3
}
fn default_retries() -> u64 {
    3
}
fn default_start_period() -> u64 {
    10
}

impl Service {
    /// 他のServiceをマージする
    ///
    /// otherで定義されたフィールドが優先される（オーバーライド）。
    /// - Option<T>: otherがSomeならそれを使用、Noneなら元の値を維持
    /// - Vec<T>: otherが空でなければそれを使用、空なら元の値を維持
    /// - HashMap<K, V>: 元の値にotherの値をマージ（otherが優先）
    pub fn merge(&mut self, other: Service) {
        // Option<T>フィールド: otherがSomeなら上書き
        if other.image.is_some() {
            self.image = other.image;
        }
        if other.version.is_some() {
            self.version = other.version;
        }
        if other.command.is_some() {
            self.command = other.command;
        }
        if other.build.is_some() {
            self.build = other.build;
        }
        if other.healthcheck.is_some() {
            self.healthcheck = other.healthcheck;
        }

        // Vec<T>フィールド: otherが空でなければ上書き
        if !other.ports.is_empty() {
            self.ports = other.ports;
        }
        if !other.volumes.is_empty() {
            self.volumes = other.volumes;
        }
        if !other.depends_on.is_empty() {
            self.depends_on = other.depends_on;
        }

        // HashMap<K, V>フィールド: マージ（otherの値が優先）
        for (key, value) in other.environment {
            self.environment.insert(key, value);
        }
    }
}
