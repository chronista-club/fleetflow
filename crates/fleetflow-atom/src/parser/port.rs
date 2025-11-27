//! ポートノードのパース

use crate::model::{Port, Protocol};
use kdl::KdlNode;

/// port ノードをパース
///
/// サポートされる形式:
/// - 名前付き引数: port host=8080 container=3000
/// - 位置引数（後方互換）: port 8080 3000
pub fn parse_port(node: &KdlNode) -> Option<Port> {
    // 名前付き引数を優先
    let host = node
        .get("host")
        .and_then(|e| e.as_integer())
        .map(|v| v as u16)
        .or_else(|| {
            // フォールバック: 位置引数
            node.entries().first()?.value().as_integer().map(|v| v as u16)
        })?;

    let container = node
        .get("container")
        .and_then(|e| e.as_integer())
        .map(|v| v as u16)
        .or_else(|| {
            // フォールバック: 位置引数
            node.entries()
                .get(1)?
                .value()
                .as_integer()
                .map(|v| v as u16)
        })?;

    let protocol = node
        .get("protocol")
        .and_then(|e| e.as_string())
        .and_then(|s| match s {
            "tcp" => Some(Protocol::Tcp),
            "udp" => Some(Protocol::Udp),
            _ => None,
        })
        .unwrap_or_default();

    let host_ip = node
        .get("host_ip")
        .and_then(|e| e.as_string())
        .map(|s| s.to_string());

    Some(Port {
        host,
        container,
        protocol,
        host_ip,
    })
}
