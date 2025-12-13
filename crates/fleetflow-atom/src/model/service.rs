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
    /// 再起動ポリシー (no, always, on-failure, unless-stopped)
    pub restart: Option<RestartPolicy>,
    /// 依存サービス待機設定（exponential backoff）
    pub wait_for: Option<WaitConfig>,
}

/// 再起動ポリシー
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    /// 再起動しない（デフォルト）
    No,
    /// 常に再起動
    Always,
    /// 異常終了時のみ再起動
    OnFailure,
    /// 明示的に停止しない限り再起動
    UnlessStopped,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::No
    }
}

impl RestartPolicy {
    /// 文字列からパース
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "no" => Some(Self::No),
            "always" => Some(Self::Always),
            "on-failure" | "on_failure" => Some(Self::OnFailure),
            "unless-stopped" | "unless_stopped" => Some(Self::UnlessStopped),
            _ => None,
        }
    }

    /// Docker APIで使用する文字列に変換
    pub fn as_docker_str(&self) -> &'static str {
        match self {
            Self::No => "no",
            Self::Always => "always",
            Self::OnFailure => "on-failure",
            Self::UnlessStopped => "unless-stopped",
        }
    }
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

/// 依存サービス待機設定（Exponential Backoff）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitConfig {
    /// 最大リトライ回数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 初期待機時間（ミリ秒）
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,
    /// 最大待機時間（ミリ秒）
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,
    /// Exponential倍率
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
}

fn default_max_retries() -> u32 {
    23
}
fn default_initial_delay() -> u64 {
    1000 // 1秒
}
fn default_max_delay() -> u64 {
    30000 // 30秒
}
fn default_multiplier() -> f64 {
    2.0
}

impl Default for WaitConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            initial_delay_ms: default_initial_delay(),
            max_delay_ms: default_max_delay(),
            multiplier: default_multiplier(),
        }
    }
}

impl WaitConfig {
    /// 指定回数目の待機時間を計算（ミリ秒）
    pub fn delay_for_attempt(&self, attempt: u32) -> u64 {
        let delay = self.initial_delay_ms as f64 * self.multiplier.powi(attempt as i32);
        (delay as u64).min(self.max_delay_ms)
    }
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
        if other.restart.is_some() {
            self.restart = other.restart;
        }
        if other.wait_for.is_some() {
            self.wait_for = other.wait_for;
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
