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
                "disk_size" | "disk-size" => {
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
                "archive" => {
                    server.archive = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "startup_script" | "startup-script" | "init_script" | "init-script" => {
                    server.startup_script = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
                }
                "init_script_vars" | "init-script-vars" => {
                    // init-script-varsブロックをパース
                    // 例: init-script-vars { SSH_PUBKEY "ssh-rsa ..." TAILSCALE_AUTHKEY "tskey-..." }
                    if let Some(vars_children) = child.children() {
                        for var_child in vars_children.nodes() {
                            let key = var_child.name().value().to_string();
                            if let Some(value) = var_child
                                .entries()
                                .first()
                                .and_then(|e| e.value().as_string())
                            {
                                server.init_script_vars.insert(key, value.to_string());
                            }
                        }
                    }
                }
                "ssh_keys" | "ssh-keys" | "ssh_key" | "ssh-key" => {
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
                "dns_alias" | "dns_aliases" | "dns-alias" | "dns-aliases" => {
                    // 複数のDNSエイリアスを引数として受け取る
                    server.dns_aliases = child
                        .entries()
                        .iter()
                        .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
                        .collect();
                }
                "dns" => {
                    // dnsブロックをパース（hostname, aliasesを含む）
                    if let Some(dns_children) = child.children() {
                        for dns_child in dns_children.nodes() {
                            match dns_child.name().value() {
                                "hostname" => {
                                    // hostnameは今のところconfigに格納
                                    if let Some(hostname) = dns_child
                                        .entries()
                                        .first()
                                        .and_then(|e| e.value().as_string())
                                    {
                                        server.config.insert(
                                            "dns_hostname".to_string(),
                                            hostname.to_string(),
                                        );
                                    }
                                }
                                "aliases" => {
                                    server.dns_aliases = dns_child
                                        .entries()
                                        .iter()
                                        .filter_map(|e| {
                                            e.value().as_string().map(|s| s.to_string())
                                        })
                                        .collect();
                                }
                                _ => {}
                            }
                        }
                    }
                }
                "deploy_path" | "deploy-path" => {
                    server.deploy_path = child
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .map(|s| s.to_string());
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

    #[test]
    fn test_parse_server_with_empty_dns_aliases() {
        let kdl = r#"
            server "creo-vps" {
                provider "sakura-cloud"
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, server) = parse_server(node).unwrap();
        // dns_aliasesが指定されていない場合は空配列
        assert!(server.dns_aliases.is_empty());
    }

    #[test]
    fn test_parse_server_with_single_dns_alias() {
        let kdl = r#"
            server "creo-vps" {
                provider "sakura-cloud"
                dns_aliases "app"
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, server) = parse_server(node).unwrap();
        assert_eq!(server.dns_aliases, vec!["app"]);
    }

    #[test]
    fn test_parse_server_with_duplicate_dns_aliases() {
        // 重複した場合もそのまま保持（重複排除はアプリケーション層の責務）
        let kdl = r#"
            server "creo-vps" {
                provider "sakura-cloud"
                dns_aliases "app" "app" "api"
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, server) = parse_server(node).unwrap();
        assert_eq!(server.dns_aliases, vec!["app", "app", "api"]);
        assert_eq!(server.dns_aliases.len(), 3);
    }

    #[test]
    fn test_parse_server_with_deploy_path() {
        let kdl = r#"
            server "creo-vps" {
                provider "sakura-cloud"
                deploy_path "/opt/creo-memories"
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (name, server) = parse_server(node).unwrap();
        assert_eq!(name, "creo-vps");
        assert_eq!(server.deploy_path, Some("/opt/creo-memories".to_string()));
    }

    #[test]
    fn test_parse_server_without_deploy_path() {
        let kdl = r#"
            server "creo-vps" {
                provider "sakura-cloud"
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (_, server) = parse_server(node).unwrap();
        assert!(server.deploy_path.is_none());
    }

    #[test]
    fn test_parse_server_full_config() {
        let kdl = r#"
            server "creo-vps" {
                provider "sakura-cloud"
                plan "4core-8gb"
                disk_size 100
                os "ubuntu-24.04"
                ssh_keys "my-key"
                dns_aliases "app" "api"
                deploy_path "/opt/myapp"
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (name, server) = parse_server(node).unwrap();
        assert_eq!(name, "creo-vps");
        assert_eq!(server.provider, "sakura-cloud");
        assert_eq!(server.plan, Some("4core-8gb".to_string()));
        assert_eq!(server.disk_size, Some(100));
        assert_eq!(server.os, Some("ubuntu-24.04".to_string()));
        assert_eq!(server.ssh_keys, vec!["my-key"]);
        assert_eq!(server.dns_aliases, vec!["app", "api"]);
        assert_eq!(server.deploy_path, Some("/opt/myapp".to_string()));
    }

    #[test]
    fn test_parse_server_kebab_case() {
        // kebab-case naming (as used in cloud.kdl)
        let kdl = r#"
            server "creo-dev" {
                provider "sakura-cloud"
                plan "4core-8gb"
                disk-size 100
                os "debian12"
                ssh-key "mito-mac.local"
                init-script "scripts/init-server.sh"
                deploy-path "/opt/creo-memories"
                dns {
                    hostname "dev"
                    aliases "forge"
                }
            }
        "#;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let node = doc.nodes().first().unwrap();

        let (name, server) = parse_server(node).unwrap();
        assert_eq!(name, "creo-dev");
        assert_eq!(server.provider, "sakura-cloud");
        assert_eq!(server.plan, Some("4core-8gb".to_string()));
        assert_eq!(server.disk_size, Some(100));
        assert_eq!(server.os, Some("debian12".to_string()));
        assert_eq!(server.ssh_keys, vec!["mito-mac.local"]);
        assert_eq!(
            server.startup_script,
            Some("scripts/init-server.sh".to_string())
        );
        assert_eq!(server.deploy_path, Some("/opt/creo-memories".to_string()));
        assert_eq!(server.dns_aliases, vec!["forge"]);
        assert_eq!(server.config.get("dns_hostname"), Some(&"dev".to_string()));
    }
}
