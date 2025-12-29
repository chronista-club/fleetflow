//! ポート定義

use serde::{Deserialize, Serialize};
use unison_kdl::{
    Error as KdlError, FromKdlValue, KdlDeserialize, KdlSerialize, KdlValue, ToKdlValue,
};

/// ポート定義
#[derive(Debug, Clone, Serialize, Deserialize, KdlDeserialize, KdlSerialize)]
#[kdl(name = "port")]
pub struct Port {
    #[kdl(property)]
    pub host: u16,
    #[kdl(property)]
    pub container: u16,
    #[serde(default = "default_protocol")]
    #[kdl(property, default)]
    pub protocol: Protocol,
    #[kdl(property)]
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

impl Protocol {
    /// 文字列からProtocolをパース
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "udp" => Protocol::Udp,
            _ => Protocol::Tcp,
        }
    }

    /// Docker APIで使用する文字列に変換
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }
}

// KDL変換の実装
impl<'de> FromKdlValue<'de> for Protocol {
    fn from_kdl_value(value: &'de KdlValue) -> unison_kdl::Result<Self> {
        value
            .as_string()
            .map(Protocol::parse)
            .ok_or_else(|| KdlError::type_mismatch("protocol string", value))
    }
}

impl ToKdlValue for Protocol {
    fn to_kdl_value(&self) -> KdlValue {
        KdlValue::String(self.as_str().to_string())
    }
}

impl ToKdlValue for &Protocol {
    fn to_kdl_value(&self) -> KdlValue {
        KdlValue::String(self.as_str().to_string())
    }
}

fn default_protocol() -> Protocol {
    Protocol::Tcp
}
