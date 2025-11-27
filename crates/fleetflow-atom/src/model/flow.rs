//! Flow定義

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
}
