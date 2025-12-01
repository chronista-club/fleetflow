//! クラウドリソースノードのパース

use crate::error::{FlowError, Result};
use crate::model::{CloudProvider, ServerResource};
use kdl::KdlNode;

/// provider ノードをパース
pub fn parse_provider(node: &KdlNode) -> Result<(String, CloudProvider)> {
    let name = node
        .entries()
        .first()
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| FlowError::InvalidConfig("provider requires a name".to_string()))?
        .to_string();

    let mut provider = CloudProvider {
        name: name.clone(),
        ..Default::default()
    };

    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "zone" => {
                    provider.zone = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                // 追加設定はconfigに保存
                other => {
                    if let Some(value) = child.entries().first().and_then(|e| e.value().as_string())
                    {
                        provider.config.insert(other.to_string(), value.to_string());
                    }
                }
            }
        }
    }

    Ok((name, provider))
}

/// server ノードをパース
pub fn parse_server(node: &KdlNode) -> Result<(String, ServerResource)> {
    let name = node
        .entries()
        .first()
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| FlowError::InvalidConfig("server requires a name".to_string()))?
        .to_string();

    let mut server = ServerResource::default();

    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "provider" => {
                    server.provider = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .unwrap_or("")
                        .to_string();
                }
                "plan" => {
                    server.plan = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "disk_size" => {
                    server.disk_size = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_integer())
                        .map(|v| v as u32);
                }
                "os" => {
                    server.os = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "startup_script" => {
                    server.startup_script = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "ssh_keys" => {
                    // 複数のSSHキーを引数として受け取る
                    server.ssh_keys = child
                        .entries()
                        .iter()
                        .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
                        .collect();
                }
                "tags" => {
                    // 複数のタグを引数として受け取る
                    server.tags = child
                        .entries()
                        .iter()
                        .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
                        .collect();
                }
                "dns_alias" | "dns_aliases" => {
                    // 複数のDNSエイリアスを引数として受け取る
                    server.dns_aliases = child
                        .entries()
                        .iter()
                        .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
                        .collect();
                }
                // 追加設定はconfigに保存
                other => {
                    if let Some(value) = child.entries().first().and_then(|e| e.value().as_string())
                    {
                        server.config.insert(other.to_string(), value.to_string());
                    }
                }
            }
        }
    }

    Ok((name, server))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_provider() {
        let kdl = r#"
            provider "sakura-cloud" {
                zone "tk1a"
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (name, provider) = parse_provider(node).unwrap();
        assert_eq!(name, "sakura-cloud");
        assert_eq!(provider.zone, Some("tk1a".to_string()));
    }

    #[test]
    fn test_parse_server() {
        let kdl = r#"
            server "creo-vps" {
                provider "sakura-cloud"
                plan "2core-4gb"
                disk_size 100
                os "ubuntu-24.04"
                ssh_keys "my-key" "another-key"
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (name, server) = parse_server(node).unwrap();
        assert_eq!(name, "creo-vps");
        assert_eq!(server.provider, "sakura-cloud");
        assert_eq!(server.plan, Some("2core-4gb".to_string()));
        assert_eq!(server.disk_size, Some(100));
        assert_eq!(server.os, Some("ubuntu-24.04".to_string()));
        assert_eq!(server.ssh_keys.len(), 2);
    }

    #[test]
    fn test_parse_server_with_dns_aliases() {
        let kdl = r#"
            server "creo-vps" {
                provider "sakura-cloud"
                plan "4core-8gb"
                dns_aliases "app" "api" "www"
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (name, server) = parse_server(node).unwrap();
        assert_eq!(name, "creo-vps");
        assert_eq!(server.provider, "sakura-cloud");
        assert_eq!(server.plan, Some("4core-8gb".to_string()));
        assert_eq!(server.dns_aliases, vec!["app", "api", "www"]);
    }
}
