//! ボリュームノードのパース

use crate::model::Volume;
use kdl::KdlNode;
use std::path::PathBuf;

/// volume ノードをパース
pub fn parse_volume(node: &KdlNode) -> Option<Volume> {
    let entries: Vec<_> = node.entries().iter().collect();

    let host = PathBuf::from(entries.first()?.value().as_string()?);
    let container = PathBuf::from(entries.get(1)?.value().as_string()?);

    // Issue #13: ブール値の改善されたパース
    let read_only = parse_bool_with_hint(node, "read_only").unwrap_or(false);

    Some(Volume {
        host,
        container,
        read_only,
    })
}

/// ブール値をパースし、`true`/`false` 文字列が使用された場合は警告を出力
/// Issue #13: KDL v2では `#true`/`#false` を使用する必要がある
fn parse_bool_with_hint(node: &KdlNode, key: &str) -> Option<bool> {
    // まず正式なブール値を試す
    if let Some(value) = node.get(key).and_then(|e| e.as_bool()) {
        return Some(value);
    }

    // 文字列 "true" / "false" が使用されていないかチェック
    if let Some(entry) = node.get(key)
        && let Some(str_value) = entry.as_string()
    {
        match str_value {
            "true" => {
                eprintln!(
                    "Warning: '{key}=\"true\"' is a string, not a boolean.\n\
                         Hint: In KDL v2, use '#true' for boolean values.\n\
                         Example: {key}=#true"
                );
                return Some(true);
            }
            "false" => {
                eprintln!(
                    "Warning: '{key}=\"false\"' is a string, not a boolean.\n\
                         Hint: In KDL v2, use '#false' for boolean values.\n\
                         Example: {key}=#false"
                );
                return Some(false);
            }
            _ => {}
        }
    }

    None
}
