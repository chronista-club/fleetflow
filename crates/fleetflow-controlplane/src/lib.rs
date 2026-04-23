pub mod agent_registry;
pub mod auth;
pub mod crypto;
pub mod db;
pub mod handlers;
pub mod log_router;
pub mod model;
pub mod server;
pub mod server_provider;

/// SurrealDB RecordId を re-export。
///
/// fleetflowd (REST handler) から build_job 等の id lookup を行う時に
/// `fleetflow_controlplane::RecordId::new("table", key)` の形で使う。
/// Controlplane が唯一の SurrealDB 依存点なので、外部 crate は本 alias 経由で参照する。
pub use surrealdb::types::RecordId;
