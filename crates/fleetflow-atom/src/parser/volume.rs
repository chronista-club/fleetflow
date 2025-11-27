//! ボリュームノードのパース

use crate::model::Volume;
use kdl::KdlNode;
use std::path::PathBuf;

/// volume ノードをパース
pub fn parse_volume(node: &KdlNode) -> Option<Volume> {
    let entries: Vec<_> = node.entries().iter().collect();

    let host = PathBuf::from(entries.first()?.value().as_string()?);
    let container = PathBuf::from(entries.get(1)?.value().as_string()?);

    let read_only = node
        .get("read_only")
        .and_then(|e| e.as_bool())
        .unwrap_or(false);

    Some(Volume {
        host,
        container,
        read_only,
    })
}
