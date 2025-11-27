//! KDLパーサー
//!
//! FleetFlowのKDL設定ファイルをパースします。
//! 各ノードタイプのパース処理はモジュールに分離されています。

mod port;
mod service;
mod stage;
mod volume;

// 内部で使用するパース関数
use service::parse_service;
use stage::parse_stage;

// テスト用にre-export
#[cfg(test)]
pub(crate) use service::infer_image_name;

use crate::error::Result;
use crate::model::Flow;
use kdl::KdlDocument;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// KDLファイルをパースしてFlowを生成
pub fn parse_kdl_file<P: AsRef<Path>>(path: P) -> Result<Flow> {
    let content = fs::read_to_string(path.as_ref())?;
    let name = path
        .as_ref()
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();
    parse_kdl_string(&content, name)
}

/// KDL文字列をパース
pub fn parse_kdl_string(content: &str, default_name: String) -> Result<Flow> {
    let doc: KdlDocument = content.parse()?;

    let mut stages = HashMap::new();
    let mut services = HashMap::new();
    let mut name = default_name;

    for node in doc.nodes() {
        match node.name().value() {
            "project" => {
                // projectノードから名前を取得
                if let Some(project_name) =
                    node.entries().first().and_then(|e| e.value().as_string())
                {
                    name = project_name.to_string();
                }
            }
            "stage" => {
                let (stage_name, stage) = parse_stage(node)?;
                stages.insert(stage_name, stage);
            }
            "service" => {
                let (service_name, service) = parse_service(node)?;
                services.insert(service_name, service);
            }
            "include" => {
                // TODO: include機能の実装
            }
            "variables" => {
                // TODO: 変数定義の実装
            }
            _ => {
                // 不明なノードはスキップ（projectなどの追加ノードも許可）
            }
        }
    }

    Ok(Flow {
        name,
        stages,
        services,
    })
}

#[cfg(test)]
mod tests;
