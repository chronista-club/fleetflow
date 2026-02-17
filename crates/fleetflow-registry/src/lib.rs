//! Fleet Registry — 複数fleetとサーバーの統合管理
//!
//! Fleet Registryは、複数のFleetFlowプロジェクト（fleet）と
//! 計算資源（サーバー）を統一的に管理する仕組みです。
//!
//! # 概要
//!
//! - **Services層**: 何を動かすか（fleet定義）
//! - **Infrastructure層**: どこで動かすか（server定義）
//! - **Deployment Routing**: どのfleetをどのサーバーにデプロイするか

pub mod discovery;
pub mod error;
pub mod model;
pub mod parser;

pub use discovery::*;
pub use error::*;
pub use model::*;
pub use parser::*;
