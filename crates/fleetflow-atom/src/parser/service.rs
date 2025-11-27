//! サービスノードのパース

use super::port::parse_port;
use super::volume::parse_volume;
use crate::error::{FlowError, Result};
use crate::model::{BuildConfig, Service};
use kdl::{KdlDocument, KdlNode};
use std::path::PathBuf;

/// service ノードをパース
pub fn parse_service(node: &KdlNode) -> Result<(String, Service)> {
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
                // ビルド関連フィールド（フラット記法）
                "dockerfile" => {
                    if let Some(path) = child.entries().first().and_then(|e| e.value().as_string())
                    {
                        service.build.get_or_insert_with(Default::default).dockerfile =
                            Some(PathBuf::from(path));
                    }
                }
                "context" => {
                    if let Some(path) = child.entries().first().and_then(|e| e.value().as_string())
                    {
                        service.build.get_or_insert_with(Default::default).context =
                            Some(PathBuf::from(path));
                    }
                }
                "target" => {
                    if let Some(target) =
                        child.entries().first().and_then(|e| e.value().as_string())
                    {
                        service.build.get_or_insert_with(Default::default).target =
                            Some(target.to_string());
                    }
                }
                "build_args" => {
                    if let Some(args) = child.children() {
                        let build_config = service.build.get_or_insert_with(Default::default);
                        for arg_node in args.nodes() {
                            let key = arg_node.name().value().to_string();
                            let value = arg_node
                                .entries()
                                .first()
                                .and_then(|e| e.value().as_string())
                                .unwrap_or("")
                                .to_string();
                            build_config.args.insert(key, value);
                        }
                    }
                }
                "image_tag" => {
                    if let Some(tag) = child.entries().first().and_then(|e| e.value().as_string())
                    {
                        service.build.get_or_insert_with(Default::default).image_tag =
                            Some(tag.to_string());
                    }
                }
                // ネストしたbuildブロック
                "build" => {
                    if let Some(build_children) = child.children() {
                        service.build = Some(parse_build_config(build_children));
                    }
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

/// buildブロックをパース（ネスト記法用）
pub fn parse_build_config(doc: &KdlDocument) -> BuildConfig {
    let mut config = BuildConfig::default();

    for node in doc.nodes() {
        match node.name().value() {
            "dockerfile" => {
                if let Some(path) = node.entries().first().and_then(|e| e.value().as_string()) {
                    config.dockerfile = Some(PathBuf::from(path));
                }
            }
            "context" => {
                if let Some(path) = node.entries().first().and_then(|e| e.value().as_string()) {
                    config.context = Some(PathBuf::from(path));
                }
            }
            "args" => {
                if let Some(args_children) = node.children() {
                    for arg_node in args_children.nodes() {
                        let key = arg_node.name().value().to_string();
                        let value = arg_node
                            .entries()
                            .first()
                            .and_then(|e| e.value().as_string())
                            .unwrap_or("")
                            .to_string();
                        config.args.insert(key, value);
                    }
                }
            }
            "target" => {
                if let Some(target) = node.entries().first().and_then(|e| e.value().as_string()) {
                    config.target = Some(target.to_string());
                }
            }
            "no_cache" => {
                if let Some(value) = node.entries().first().and_then(|e| e.value().as_bool()) {
                    config.no_cache = value;
                }
            }
            "image_tag" => {
                if let Some(tag) = node.entries().first().and_then(|e| e.value().as_string()) {
                    config.image_tag = Some(tag.to_string());
                }
            }
            _ => {}
        }
    }

    config
}

/// サービス名からイメージ名を推測
pub fn infer_image_name(service_name: &str, version: Option<&str>) -> String {
    let tag = version.unwrap_or("latest");
    format!("{}:{}", service_name, tag)
}
