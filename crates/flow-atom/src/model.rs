use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Flow設定のルート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowConfig {
    pub environments: HashMap<String, Environment>,
    pub services: HashMap<String, Service>,
}

/// 環境定義
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Environment {
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

/// サービス定義
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Service {
    pub image: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    pub volumes: Vec<Volume>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// ポート定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub host: u16,
    pub container: u16,
    #[serde(default = "default_protocol")]
    pub protocol: Protocol,
    pub host_ip: Option<String>,
}

/// プロトコル種別
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Tcp,
    Udp,
}

fn default_protocol() -> Protocol {
    Protocol::Tcp
}

/// ボリューム定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub host: PathBuf,
    pub container: PathBuf,
    #[serde(default)]
    pub read_only: bool,
}
