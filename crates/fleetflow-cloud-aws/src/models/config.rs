//! AWS 設定モデル（KDL パース結果）

use serde::{Deserialize, Serialize};

use super::{SecurityGroupConfig, SubnetConfig};

/// AWS クラウド設定（KDL の `cloud "aws" { ... }` ブロック全体）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AwsConfig {
    /// リージョン（e.g., "ap-northeast-1"）
    pub region: String,

    /// 既存 VPC ID（e.g., "vpc-abc123"）
    pub vpc_id: String,

    /// Subnet 定義
    pub subnets: Vec<SubnetConfig>,

    /// Security Group 定義
    pub security_groups: Vec<SecurityGroupConfig>,

    /// サーバー定義
    pub servers: Vec<AwsServerConfig>,
}

/// AWS サーバー設定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AwsServerConfig {
    /// サーバー名
    pub name: String,

    /// 参照する Subnet 名
    pub subnet: String,

    /// 参照する Security Group 名
    pub security_group: String,

    /// Elastic IP を割り当てるか
    pub elastic_ip: bool,

    /// CPU コア数
    pub cpu: i32,

    /// メモリ（GB）
    pub memory_gb: i32,

    /// OS（e.g., "ubuntu-24.04"）
    pub os: String,

    /// Key Pair 名（既存）
    pub key_pair: String,
}

impl AwsConfig {
    /// 設定全体のバリデーション
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.region.is_empty() {
            errors.push("region is required".to_string());
        }

        if self.vpc_id.is_empty() {
            errors.push("vpc ID is required".to_string());
        }

        // Subnet バリデーション
        let subnet_names: Vec<&str> = self.subnets.iter().map(|s| s.name.as_str()).collect();
        for subnet in &self.subnets {
            if let Err(e) = subnet.validate() {
                errors.push(format!("subnet '{}': {e}", subnet.name));
            }
        }

        // SG バリデーション
        let sg_names: Vec<&str> = self
            .security_groups
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        for sg in &self.security_groups {
            if let Err(e) = sg.validate() {
                errors.push(format!("security-group '{}': {e}", sg.name));
            }
        }

        // サーバーの参照整合性チェック
        for server in &self.servers {
            if !subnet_names.contains(&server.subnet.as_str()) {
                errors.push(format!(
                    "server '{}': subnet '{}' is not defined",
                    server.name, server.subnet
                ));
            }
            if !sg_names.contains(&server.security_group.as_str()) {
                errors.push(format!(
                    "server '{}': security-group '{}' is not defined",
                    server.name, server.security_group
                ));
            }
            if server.key_pair.is_empty() {
                errors.push(format!("server '{}': key-pair is required", server.name));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::security_group::{PortSpec, SecurityGroupRule};

    fn valid_config() -> AwsConfig {
        AwsConfig {
            region: "ap-northeast-1".into(),
            vpc_id: "vpc-abc123".into(),
            subnets: vec![SubnetConfig {
                name: "web".into(),
                cidr: "10.0.1.0/24".into(),
                az: "ap-northeast-1a".into(),
            }],
            security_groups: vec![SecurityGroupConfig {
                name: "web-sg".into(),
                inbound_rules: vec![SecurityGroupRule {
                    protocol: "tcp".into(),
                    port: PortSpec::Single(80),
                    from: "0.0.0.0/0".into(),
                }],
            }],
            servers: vec![AwsServerConfig {
                name: "web-01".into(),
                subnet: "web".into(),
                security_group: "web-sg".into(),
                elastic_ip: true,
                cpu: 2,
                memory_gb: 4,
                os: "ubuntu-24.04".into(),
                key_pair: "my-key".into(),
            }],
        }
    }

    #[test]
    fn test_valid_config() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn test_empty_region() {
        let mut config = valid_config();
        config.region = "".into();
        let errs = config.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("region")));
    }

    #[test]
    fn test_empty_vpc_id() {
        let mut config = valid_config();
        config.vpc_id = "".into();
        let errs = config.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("vpc")));
    }

    #[test]
    fn test_server_references_undefined_subnet() {
        let mut config = valid_config();
        config.servers[0].subnet = "nonexistent".into();
        let errs = config.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.contains("subnet 'nonexistent' is not defined"))
        );
    }

    #[test]
    fn test_server_references_undefined_sg() {
        let mut config = valid_config();
        config.servers[0].security_group = "nonexistent".into();
        let errs = config.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.contains("security-group 'nonexistent' is not defined"))
        );
    }

    #[test]
    fn test_server_empty_key_pair() {
        let mut config = valid_config();
        config.servers[0].key_pair = "".into();
        let errs = config.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("key-pair")));
    }

    #[test]
    fn test_multiple_errors() {
        let mut config = valid_config();
        config.region = "".into();
        config.vpc_id = "".into();
        config.servers[0].subnet = "bad".into();
        let errs = config.validate().unwrap_err();
        assert!(errs.len() >= 3);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let config = valid_config();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AwsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }
}
