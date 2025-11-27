//! ポート定義

use serde::{Deserialize, Serialize};

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
