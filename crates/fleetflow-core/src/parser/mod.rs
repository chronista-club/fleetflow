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

use crate::error::{FlowError, Result};
use crate::model::{Flow, Service};
use crate::template::{TemplateProcessor, extract_variables};
use kdl::KdlDocument;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// KDLファイルをパースしてFlowを生成（include展開・変数展開対応）
pub fn parse_kdl_file<P: AsRef<Path>>(path: P) -> Result<Flow> {
    let mut visited = HashSet::new();
    let base_dir = path
        .as_ref()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    // include ディレクティブを再帰的に展開
    let content = read_kdl_with_includes(path.as_ref(), &base_dir, &mut visited)?;

    let name = path
        .as_ref()
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();

    parse_kdl_with_variables(&content, name)
}

/// includeディレクティブを展開してKDLコンテンツを読み込む
fn read_kdl_with_includes(
    path: &Path,
    base_dir: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<String> {
    // 絶対パスに変換
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir
            .join(path)
            .canonicalize()
            .map_err(|e| FlowError::IoError {
                path: path.to_path_buf(),
                message: format!("パス解決エラー: {}", e),
            })?
    };

    // 循環参照チェック
    if visited.contains(&abs_path) {
        return Err(FlowError::InvalidConfig(format!(
            "Circular include detected: {}",
            abs_path.display()
        )));
    }
    visited.insert(abs_path.clone());

    // ファイルを読み込む
    let content = fs::read_to_string(&abs_path).map_err(|e| FlowError::IoError {
        path: abs_path.clone(),
        message: e.to_string(),
    })?;

    // KDLドキュメントをパースしてincludeノードを処理
    let doc: KdlDocument = content.parse().map_err(|e| {
        FlowError::InvalidConfig(format!("KDL parse error in {}: {}", abs_path.display(), e))
    })?;

    let mut result = String::new();
    let current_dir = abs_path.parent().unwrap_or(base_dir);

    for node in doc.nodes() {
        if node.name().value() == "include" {
            if let Some(include_path) = node.entries().first().and_then(|e| e.value().as_string()) {
                if include_path.contains('*') {
                    // グロブパターンで展開
                    let pattern = current_dir.join(include_path);
                    let pattern_str = pattern.to_str().ok_or_else(|| {
                        FlowError::InvalidConfig(format!(
                            "Invalid include pattern: {}",
                            include_path
                        ))
                    })?;

                    for entry in glob::glob(pattern_str).map_err(|e| {
                        FlowError::InvalidConfig(format!("Invalid glob pattern: {}", e))
                    })? {
                        let entry_path = entry
                            .map_err(|e| FlowError::InvalidConfig(format!("Glob error: {}", e)))?;
                        let included = read_kdl_with_includes(&entry_path, current_dir, visited)?;
                        result.push_str(&included);
                        result.push('\n');
                    }
                } else {
                    // 単一ファイル
                    let include_file = current_dir.join(include_path);
                    let included = read_kdl_with_includes(&include_file, current_dir, visited)?;
                    result.push_str(&included);
                    result.push('\n');
                }
            }
        } else {
            // include以外のノードはそのまま保持
            result.push_str(&node.to_string());
            result.push('\n');
        }
    }

    Ok(result)
}

/// 変数展開をサポートするKDLパーサー
fn parse_kdl_with_variables(content: &str, default_name: String) -> Result<Flow> {
    // 1. variablesブロックから変数を抽出
    let variables = extract_variables(content)?;

    // 2. 変数がある場合はテンプレート展開
    let expanded = if !variables.is_empty() {
        let mut processor = TemplateProcessor::new();
        processor.add_variables(variables);
        processor.add_env_variables();
        processor.render_str(content)?
    } else {
        content.to_string()
    };

    // 3. 展開後のKDLをパース
    parse_kdl_string_raw(&expanded, default_name)
}

/// KDL文字列をパース（変数展開あり）
pub fn parse_kdl_string(content: &str, default_name: String) -> Result<Flow> {
    parse_kdl_with_variables(content, default_name)
}

/// KDL文字列をステージ指定でパース
pub fn parse_kdl_string_with_stage(
    content: &str,
    default_name: String,
    target_stage: Option<&str>,
) -> Result<Flow> {
    // 変数展開を適用してからステージ指定パース
    let variables = extract_variables(content)?;
    let expanded = if !variables.is_empty() {
        let mut processor = TemplateProcessor::new();
        processor.add_variables(variables);
        processor.add_env_variables();
        processor.render_str(content)?
    } else {
        content.to_string()
    };
    parse_kdl_string_raw_with_stage(&expanded, default_name, target_stage)
}

/// KDL文字列を直接パース（変数展開なし）
fn parse_kdl_string_raw(content: &str, default_name: String) -> Result<Flow> {
    parse_kdl_string_raw_with_stage(content, default_name, None)
}

/// KDL文字列をステージ指定で直接パース（変数展開なし）
fn parse_kdl_string_raw_with_stage(
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
                // parse_kdl_file() 経由の場合は read_kdl_with_includes() で既に展開済み
                // parse_kdl_string() 直接呼び出しの場合はスキップ
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
