//! サービス定義

use super::port::Port;
use super::volume::Volume;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use unison_kdl::{
    Error as KdlError, FromKdlValue, KdlDeserialize, KdlSerialize, KdlValue, ToKdlValue,
};

/// サービス定義
///
/// KDL形式：
/// ```kdl
/// service "name" image="..." restart="unless-stopped" {
///     port host=8080 container=80
///     volume host="/data" container="/data"
///     env {
///         KEY "value"
///     }
/// }
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, KdlDeserialize, KdlSerialize)]
#[kdl(name = "service")]
pub struct Service {
    #[kdl(property)]
    pub image: Option<String>,
    #[kdl(property)]
    pub version: Option<String>,
    #[kdl(property)]
    pub command: Option<String>,
    #[serde(default)]
    #[kdl(children, name = "port")]
    pub ports: Vec<Port>,
    #[serde(default)]
    #[kdl(child_map, name = "env")]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    #[kdl(children, name = "volume")]
    pub volumes: Vec<Volume>,
    #[serde(default)]
    #[kdl(skip)] // depends_onは別ノードなのでスキップ
    pub depends_on: Vec<String>,
    /// ビルド設定
    #[kdl(child)]
    pub build: Option<BuildConfig>,
    /// ヘルスチェック設定
    #[kdl(child)]
    pub healthcheck: Option<HealthCheck>,
    /// 再起動ポリシー (no, always, on-failure, unless-stopped)
    #[kdl(property)]
    pub restart: Option<RestartPolicy>,
    /// 依存サービス待機設定（exponential backoff）
    #[kdl(child)]
    pub wait_for: Option<WaitConfig>,
    /// サービス固有のコンテナレジストリURL（例: ghcr.io/owner）
    #[kdl(property)]
    pub registry: Option<String>,
}

/// 再起動ポリシー
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    /// 再起動しない（デフォルト）
    #[default]
    No,
    /// 常に再起動
    Always,
    /// 異常終了時のみ再起動
    OnFailure,
    /// 明示的に停止しない限り再起動
    UnlessStopped,
}

impl RestartPolicy {
    /// 文字列からパース
    pub fn parse(s: &str) -> Option<Self> {
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

// KDL変換の実装
impl<'de> FromKdlValue<'de> for RestartPolicy {
    fn from_kdl_value(value: &'de KdlValue) -> unison_kdl::Result<Self> {
        value
            .as_string()
            .and_then(Self::parse)
            .ok_or_else(|| KdlError::type_mismatch("restart policy string", value))
    }
}

impl ToKdlValue for RestartPolicy {
    fn to_kdl_value(&self) -> KdlValue {
        KdlValue::String(self.as_docker_str().to_string())
    }
}

impl ToKdlValue for &RestartPolicy {
    fn to_kdl_value(&self) -> KdlValue {
        KdlValue::String(self.as_docker_str().to_string())
    }
}

/// ビルド設定
#[derive(Debug, Clone, Default, Serialize, Deserialize, KdlDeserialize, KdlSerialize)]
#[kdl(name = "build")]
pub struct BuildConfig {
    /// Dockerfileのパス（プロジェクトルートからの相対パス）
    #[kdl(property)]
    pub dockerfile: Option<PathBuf>,
    /// ビルドコンテキストのパス（プロジェクトルートからの相対パス）
    /// 未指定の場合はプロジェクトルート
    #[kdl(property)]
    pub context: Option<PathBuf>,
    /// ビルド引数
    #[serde(default)]
    #[kdl(child_map, name = "args")]
    pub args: HashMap<String, String>,
    /// マルチステージビルドのターゲット
    #[kdl(property)]
    pub target: Option<String>,
    /// キャッシュ無効化フラグ
    #[serde(default)]
    #[kdl(property, default)]
    pub no_cache: bool,
    /// イメージタグの明示的指定
    #[kdl(property)]
    pub image_tag: Option<String>,
}

/// ヘルスチェック設定
///
/// KDL形式：
/// ```kdl
/// healthcheck "CMD-SHELL" "curl -f http://localhost/health" interval=30 timeout=3
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, KdlDeserialize, KdlSerialize)]
#[kdl(name = "healthcheck")]
pub struct HealthCheck {
    /// テストコマンド (CMD-SHELL形式またはCMD形式)
    #[kdl(arguments)]
    pub test: Vec<String>,
    /// チェック間隔（秒）
    #[serde(default = "default_interval")]
    #[kdl(property, default)]
    pub interval: u64,
    /// タイムアウト（秒）
    #[serde(default = "default_timeout")]
    #[kdl(property, default)]
    pub timeout: u64,
    /// リトライ回数
    #[serde(default = "default_retries")]
    #[kdl(property, default)]
    pub retries: u64,
    /// 起動待機時間（秒）
    #[serde(default = "default_start_period")]
    #[kdl(property, default)]
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
///
/// KDL形式：
/// ```kdl
/// wait_for max_retries=23 initial_delay_ms=1000 max_delay_ms=30000 multiplier=2.0
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, KdlDeserialize, KdlSerialize)]
#[kdl(name = "wait_for")]
pub struct WaitConfig {
    /// 最大リトライ回数
    #[serde(default = "default_max_retries")]
    #[kdl(property, default)]
    pub max_retries: u32,
    /// 初期待機時間（ミリ秒）
    #[serde(default = "default_initial_delay")]
    #[kdl(property, default)]
    pub initial_delay_ms: u64,
    /// 最大待機時間（ミリ秒）
    #[serde(default = "default_max_delay")]
    #[kdl(property, default)]
    pub max_delay_ms: u64,
    /// Exponential倍率
    #[serde(default = "default_multiplier")]
    #[kdl(property, default)]
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
        if other.registry.is_some() {
            self.registry = other.registry;
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
