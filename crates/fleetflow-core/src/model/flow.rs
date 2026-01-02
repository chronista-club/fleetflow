//! Flow定義

use super::cloud::{CloudProvider, ServerResource};
use super::service::Service;
use super::stage::Stage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// クラウドプロバイダー設定
    #[serde(default)]
    pub providers: HashMap<String, CloudProvider>,
    /// サーバーリソース
    #[serde(default)]
    pub servers: HashMap<String, ServerResource>,
    /// デフォルトのコンテナレジストリURL（例: ghcr.io/owner）
    #[serde(default)]
    pub registry: Option<String>,
    /// プロジェクト共通の変数（全ステージで使用可能）
    #[serde(default)]
    pub variables: HashMap<String, String>,
}
