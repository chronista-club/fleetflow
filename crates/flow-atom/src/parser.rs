use crate::error::Result;
use crate::model::FlowConfig;
use std::path::Path;

/// KDLファイルをパースしてFlowConfigを生成
pub fn parse_kdl_file<P: AsRef<Path>>(_path: P) -> Result<FlowConfig> {
    // TODO: KDLパーサーの実装
    todo!("KDL parser implementation")
}

/// KDL文字列をパース
pub fn parse_kdl_string(_content: &str) -> Result<FlowConfig> {
    // TODO: KDLパーサーの実装
    todo!("KDL parser implementation")
}
