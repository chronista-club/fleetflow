//! Subnet モデル

use serde::{Deserialize, Serialize};

/// Subnet 設定（KDL からパース）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubnetConfig {
    /// Subnet 名（KDL 上の識別子）
    pub name: String,

    /// CIDR ブロック（e.g., "10.0.1.0/24"）
    pub cidr: String,

    /// Availability Zone（e.g., "ap-northeast-1a"）
    pub az: String,
}

impl SubnetConfig {
    /// CIDR の基本的なバリデーション
    pub fn validate(&self) -> Result<(), String> {
        // CIDR 形式: x.x.x.x/n
        let parts: Vec<&str> = self.cidr.split('/').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid CIDR format: {}", self.cidr));
        }

        let octets: Vec<&str> = parts[0].split('.').collect();
        if octets.len() != 4 {
            return Err(format!("Invalid IP in CIDR: {}", parts[0]));
        }

        for octet in &octets {
            if octet.parse::<u8>().is_err() {
                return Err(format!("Invalid octet in CIDR: {octet}"));
            }
        }

        let prefix: u8 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid prefix length: {}", parts[1]))?;
        if prefix > 32 {
            return Err(format!("Prefix length out of range: {prefix}"));
        }

        if self.az.is_empty() {
            return Err("Availability zone is required".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subnet_config_valid() {
        let subnet = SubnetConfig {
            name: "web".into(),
            cidr: "10.0.1.0/24".into(),
            az: "ap-northeast-1a".into(),
        };
        assert!(subnet.validate().is_ok());
    }

    #[test]
    fn test_subnet_config_invalid_cidr_no_prefix() {
        let subnet = SubnetConfig {
            name: "web".into(),
            cidr: "10.0.1.0".into(),
            az: "ap-northeast-1a".into(),
        };
        assert!(subnet.validate().is_err());
    }

    #[test]
    fn test_subnet_config_invalid_cidr_bad_octet() {
        let subnet = SubnetConfig {
            name: "web".into(),
            cidr: "10.0.999.0/24".into(),
            az: "ap-northeast-1a".into(),
        };
        assert!(subnet.validate().is_err());
    }

    #[test]
    fn test_subnet_config_invalid_prefix_too_large() {
        let subnet = SubnetConfig {
            name: "web".into(),
            cidr: "10.0.1.0/33".into(),
            az: "ap-northeast-1a".into(),
        };
        assert!(subnet.validate().is_err());
    }

    #[test]
    fn test_subnet_config_empty_az() {
        let subnet = SubnetConfig {
            name: "web".into(),
            cidr: "10.0.1.0/24".into(),
            az: "".into(),
        };
        assert!(subnet.validate().is_err());
    }

    #[test]
    fn test_subnet_config_serialization() {
        let subnet = SubnetConfig {
            name: "web".into(),
            cidr: "10.0.1.0/24".into(),
            az: "ap-northeast-1a".into(),
        };
        let json = serde_json::to_string(&subnet).unwrap();
        let deserialized: SubnetConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(subnet, deserialized);
    }
}
