//! Security Group モデル

use serde::{Deserialize, Serialize};

/// Security Group 設定（KDL からパース）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityGroupConfig {
    /// SG 名（KDL 上の識別子）
    pub name: String,

    /// インバウンドルール
    pub inbound_rules: Vec<SecurityGroupRule>,
}

/// Security Group ルール
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityGroupRule {
    /// プロトコル: "tcp", "udp", "icmp", "-1"(all)
    pub protocol: String,

    /// ポート指定
    pub port: PortSpec,

    /// ソース CIDR or SG ID
    pub from: String,
}

/// ポート指定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PortSpec {
    /// 単一ポート
    Single(u16),
    /// ポート範囲
    Range(u16, u16),
    /// 全ポート（ICMP や -1 の場合）
    All,
}

impl SecurityGroupConfig {
    /// バリデーション
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Security group name is required".to_string());
        }
        for (i, rule) in self.inbound_rules.iter().enumerate() {
            rule.validate()
                .map_err(|e| format!("inbound rule {}: {e}", i + 1))?;
        }
        Ok(())
    }
}

impl SecurityGroupRule {
    /// ルールのバリデーション
    pub fn validate(&self) -> Result<(), String> {
        // プロトコル検証
        match self.protocol.as_str() {
            "tcp" | "udp" => {
                // TCP/UDP はポート必須（All は不可）
                if self.port == PortSpec::All {
                    return Err(format!(
                        "{} requires a port specification",
                        self.protocol
                    ));
                }
            }
            "icmp" | "-1" => {
                // ICMP / All protocol: ポートは All であるべき
            }
            other => return Err(format!("Unknown protocol: {other}")),
        }

        // ポート範囲検証
        if let PortSpec::Range(from, to) = self.port {
            if from > to {
                return Err(format!("Invalid port range: {from}-{to}"));
            }
        }

        // from 検証（空でないこと）
        if self.from.is_empty() {
            return Err("'from' (source CIDR) is required".to_string());
        }

        Ok(())
    }
}

impl std::fmt::Display for PortSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortSpec::Single(p) => write!(f, "{p}"),
            PortSpec::Range(from, to) => write!(f, "{from}-{to}"),
            PortSpec::All => write!(f, "all"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tcp_rule(port: u16, from: &str) -> SecurityGroupRule {
        SecurityGroupRule {
            protocol: "tcp".into(),
            port: PortSpec::Single(port),
            from: from.into(),
        }
    }

    #[test]
    fn test_valid_tcp_rule() {
        let rule = tcp_rule(80, "0.0.0.0/0");
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_valid_port_range() {
        let rule = SecurityGroupRule {
            protocol: "tcp".into(),
            port: PortSpec::Range(8000, 8080),
            from: "10.0.0.0/8".into(),
        };
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_invalid_port_range_reversed() {
        let rule = SecurityGroupRule {
            protocol: "tcp".into(),
            port: PortSpec::Range(8080, 8000),
            from: "0.0.0.0/0".into(),
        };
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_tcp_requires_port() {
        let rule = SecurityGroupRule {
            protocol: "tcp".into(),
            port: PortSpec::All,
            from: "0.0.0.0/0".into(),
        };
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_icmp_allows_all_ports() {
        let rule = SecurityGroupRule {
            protocol: "icmp".into(),
            port: PortSpec::All,
            from: "0.0.0.0/0".into(),
        };
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_all_protocol() {
        let rule = SecurityGroupRule {
            protocol: "-1".into(),
            port: PortSpec::All,
            from: "10.0.0.0/8".into(),
        };
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_unknown_protocol() {
        let rule = SecurityGroupRule {
            protocol: "ftp".into(),
            port: PortSpec::Single(21),
            from: "0.0.0.0/0".into(),
        };
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_empty_from() {
        let rule = SecurityGroupRule {
            protocol: "tcp".into(),
            port: PortSpec::Single(80),
            from: "".into(),
        };
        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_security_group_validate() {
        let sg = SecurityGroupConfig {
            name: "web-sg".into(),
            inbound_rules: vec![
                tcp_rule(80, "0.0.0.0/0"),
                tcp_rule(443, "0.0.0.0/0"),
                tcp_rule(22, "10.0.0.0/8"),
            ],
        };
        assert!(sg.validate().is_ok());
    }

    #[test]
    fn test_security_group_empty_name() {
        let sg = SecurityGroupConfig {
            name: "".into(),
            inbound_rules: vec![],
        };
        assert!(sg.validate().is_err());
    }

    #[test]
    fn test_security_group_invalid_rule() {
        let sg = SecurityGroupConfig {
            name: "web-sg".into(),
            inbound_rules: vec![SecurityGroupRule {
                protocol: "tcp".into(),
                port: PortSpec::All,
                from: "0.0.0.0/0".into(),
            }],
        };
        let err = sg.validate().unwrap_err();
        assert!(err.contains("inbound rule 1"));
    }

    #[test]
    fn test_port_spec_display() {
        assert_eq!(PortSpec::Single(80).to_string(), "80");
        assert_eq!(PortSpec::Range(8000, 8080).to_string(), "8000-8080");
        assert_eq!(PortSpec::All.to_string(), "all");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let sg = SecurityGroupConfig {
            name: "web-sg".into(),
            inbound_rules: vec![
                tcp_rule(80, "0.0.0.0/0"),
                SecurityGroupRule {
                    protocol: "tcp".into(),
                    port: PortSpec::Range(8000, 8080),
                    from: "10.0.0.0/8".into(),
                },
            ],
        };
        let json = serde_json::to_string(&sg).unwrap();
        let deserialized: SecurityGroupConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(sg, deserialized);
    }
}
