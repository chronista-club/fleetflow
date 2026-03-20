//! FleetFlow AWS Cloud Provider
//!
//! AWS (EC2 / Subnet / Security Group / Elastic IP) を KDL で宣言的に管理する。
//! VPC は既存リソースを ID 指定、Subnet / SG / EC2 / EIP は FleetFlow が作成・管理。

pub mod error;
pub mod instance_type;
pub mod models;

pub use error::AwsError;
pub use models::{
    AwsConfig, AwsServerConfig, PortSpec, SecurityGroupConfig, SecurityGroupRule, SubnetConfig,
};
