//! KDLパーサー
//!
//! FleetFlowのKDL設定ファイルをパースします。
//! 各ノードタイプのパース処理はモジュールに分離されています。

mod cloud;
mod port;
mod service;
mod stage;
mod volume;

// 内部で使用するパース関数
use cloud::parse_provider;
use service::parse_service;
use stage::parse_stage;

// 外部クレートから再利用可能なパース関数
pub use cloud::parse_server;

use crate::error::Result;
use crate::model::{Flow, Service};
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
    parse_kdl_string_with_stage(content, default_name, None)
}

/// KDL文字列をステージ指定でパース
pub fn parse_kdl_string_with_stage(
    content: &str,
    default_name: String,
    target_stage: Option<&str>,
) -> Result<Flow> {
    let doc: KdlDocument = content.parse()?;

    let mut stages = HashMap::new();
    let mut services: HashMap<String, Service> = HashMap::new();
    let mut stage_service_overrides: HashMap<String, HashMap<String, Service>> = HashMap::new();
    let mut providers = HashMap::new();
    let mut servers = HashMap::new();
    let mut variables: HashMap<String, String> = HashMap::new();
    let mut name = default_name;
    let mut registry: Option<String> = None;

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
                let (stage_name, stage, stage_services) = parse_stage(node)?;
                stages.insert(stage_name.clone(), stage);

                // ステージ内で定義されたサービスを保存（後で適用）
                if !stage_services.is_empty() {
                    stage_service_overrides.insert(stage_name, stage_services);
                }
            }
            "service" => {
                let (service_name, service) = parse_service(node)?;
                // 既存のサービスがあればマージ、なければ挿入
                if let Some(existing) = services.get_mut(&service_name) {
                    existing.merge(service);
                } else {
                    services.insert(service_name, service);
                }
            }
            "provider" => {
                let (provider_name, provider) = parse_provider(node)?;
                providers.insert(provider_name, provider);
            }
            "server" => {
                let (server_name, server) = parse_server(node)?;
                servers.insert(server_name, server);
            }
            "include" => {
                // TODO: include機能の実装
            }
            "variables" => {
                // プロジェクトレベルの共通変数
                if let Some(vars) = node.children() {
                    for var in vars.nodes() {
                        let key = var.name().value().to_string();
                        let value = var
                            .entries()
                            .first()
                            .and_then(|e| e.value().as_string())
                            .unwrap_or("")
                            .to_string();
                        variables.insert(key, value);
                    }
                }
            }
            "registry" => {
                // トップレベルのレジストリURL設定
                if let Some(reg) = node.entries().first().and_then(|e| e.value().as_string()) {
                    registry = Some(reg.to_string());
                }
            }
            _ => {
                // 不明なノードはスキップ（projectなどの追加ノードも許可）
            }
        }
    }

    // ステージが指定されている場合、そのステージのサービスオーバーライドを適用
    if let Some(stage) = target_stage
        && let Some(overrides) = stage_service_overrides.get(stage)
    {
        for (service_name, service) in overrides {
            if let Some(existing) = services.get_mut(service_name) {
                existing.merge(service.clone());
            } else {
                services.insert(service_name.clone(), service.clone());
            }
        }
    }

    // Note: imageのバリデーションはstageフィルタリング後に行う
    // （buildのみ指定されたサービスがstageに含まれない場合のエラーを防ぐため）

    Ok(Flow {
        name,
        stages,
        services,
        providers,
        servers,
        registry,
        variables,
    })
}

#[cfg(test)]
mod tests;
