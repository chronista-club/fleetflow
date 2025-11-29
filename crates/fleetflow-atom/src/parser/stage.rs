//! ステージノードのパース

use crate::error::{FlowError, Result};
use crate::model::Stage;
use kdl::KdlNode;

/// stage ノードをパース
pub fn parse_stage(node: &KdlNode) -> Result<(String, Stage)> {
    let name = node
        .entries()
        .first()
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| FlowError::InvalidConfig("stage requires a name".to_string()))?
        .to_string();

    let mut stage = Stage::default();

    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "service" => {
                    // service "name" 形式で個別に指定
                    if let Some(service_name) =
                        child.entries().first().and_then(|e| e.value().as_string())
                    {
                        stage.services.push(service_name.to_string());
                    }
                }
                "server" => {
                    // server "name" 形式でサーバーを指定
                    if let Some(server_name) =
                        child.entries().first().and_then(|e| e.value().as_string())
                    {
                        stage.servers.push(server_name.to_string());
                    }
                }
                "variables" => {
                    if let Some(vars) = child.children() {
                        for var in vars.nodes() {
                            let key = var.name().value().to_string();
                            let value = var
                                .entries()
                                .first()
                                .and_then(|e| e.value().as_string())
                                .unwrap_or("")
                                .to_string();
                            stage.variables.insert(key, value);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok((name, stage))
}
