use crate::error::{FlowError, Result};
use crate::model::*;
use crate::template::{TemplateProcessor, extract_variables};
use kdl::{KdlDocument, KdlNode};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// KDLファイルをパースしてFlowを生成（変数展開・include対応）
pub fn parse_kdl_file<P: AsRef<Path>>(path: P) -> Result<Flow> {
    let mut visited = HashSet::new();
    let base_dir = path
        .as_ref()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

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
                message: format!("Failed to resolve path: {}", e),
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
            // includeノードを処理
            if let Some(include_path) = node.entries().first().and_then(|e| e.value().as_string()) {
                // グロブパターンをチェック
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

                        let included_content =
                            read_kdl_with_includes(&entry_path, current_dir, visited)?;
                        result.push_str(&included_content);
                        result.push('\n');
                    }
                } else {
                    // 単一ファイル
                    let include_file = current_dir.join(include_path);
                    let included_content =
                        read_kdl_with_includes(&include_file, current_dir, visited)?;
                    result.push_str(&included_content);
                    result.push('\n');
                }
            }
        } else {
            // includeノード以外はそのまま追加
            result.push_str(&node.to_string());
            result.push('\n');
        }
    }

    Ok(result)
}

/// KDL文字列をパース（変数展開をサポート）
pub fn parse_kdl_string(content: &str, default_name: String) -> Result<Flow> {
    parse_kdl_with_variables(content, default_name)
}

/// 変数展開をサポートするKDLパーサー
pub fn parse_kdl_with_variables(content: &str, default_name: String) -> Result<Flow> {
    // 1. variablesノードから変数を抽出
    let variables = extract_variables(content)?;

    // 2. 変数がある場合はテンプレート展開
    let expanded_content = if !variables.is_empty() {
        let mut processor = TemplateProcessor::new();
        processor.add_variables(variables);
        processor.add_env_variables(); // 環境変数も追加
        processor.render_str(content)?
    } else {
        content.to_string()
    };

    // 3. 展開後のKDLをパース
    parse_kdl_string_raw(&expanded_content, default_name)
}

/// KDL文字列を直接パース（内部使用・変数展開なし）
fn parse_kdl_string_raw(content: &str, default_name: String) -> Result<Flow> {
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

/// stage ノードをパース
fn parse_stage(node: &KdlNode) -> Result<(String, Stage)> {
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

/// service ノードをパース
fn parse_service(node: &KdlNode) -> Result<(String, Service)> {
    let name = node
        .entries()
        .first()
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| FlowError::InvalidConfig("service requires a name".to_string()))?
        .to_string();

    let mut service = Service::default();

    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "image" => {
                    service.image = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "version" => {
                    service.version = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "command" => {
                    service.command = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "ports" => {
                    if let Some(ports) = child.children() {
                        for port_node in ports.nodes() {
                            if port_node.name().value() == "port"
                                && let Some(port) = parse_port(port_node)
                            {
                                service.ports.push(port);
                            }
                        }
                    }
                }
                "environment" => {
                    if let Some(envs) = child.children() {
                        for env_node in envs.nodes() {
                            let key = env_node.name().value().to_string();
                            let value = env_node
                                .entries()
                                .first()
                                .and_then(|e| e.value().as_string())
                                .unwrap_or("")
                                .to_string();
                            service.environment.insert(key, value);
                        }
                    }
                }
                "volumes" => {
                    if let Some(vols) = child.children() {
                        for vol_node in vols.nodes() {
                            if vol_node.name().value() == "volume"
                                && let Some(volume) = parse_volume(vol_node)
                            {
                                service.volumes.push(volume);
                            }
                        }
                    }
                }
                "depends_on" => {
                    service.depends_on = child
                        .entries()
                        .iter()
                        .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
                        .collect();
                }
                _ => {}
            }
        }
    }

    // イメージ名の自動推測
    if service.image.is_none() {
        service.image = Some(infer_image_name(&name, service.version.as_deref()));
    }

    Ok((name, service))
}

/// port ノードをパース
///
/// サポートされる形式:
/// - 名前付き引数: port host=8080 container=3000
/// - 位置引数（後方互換）: port 8080 3000
fn parse_port(node: &KdlNode) -> Option<Port> {
    // 名前付き引数を優先
    let host = node
        .get("host")
        .and_then(|e| e.as_integer())
        .map(|v| v as u16)
        .or_else(|| {
            // フォールバック: 位置引数
            node.entries()
                .first()?
                .value()
                .as_integer()
                .map(|v| v as u16)
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

/// volume ノードをパース
fn parse_volume(node: &KdlNode) -> Option<Volume> {
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

/// サービス名からイメージ名を推測
fn infer_image_name(service_name: &str, version: Option<&str>) -> String {
    let tag = version.unwrap_or("latest");
    format!("{}:{}", service_name, tag)
}

#[cfg(test)]
mod tests;
