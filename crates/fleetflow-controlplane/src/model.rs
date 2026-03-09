use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

// ─────────────────────────────────────────────
// CP-001: Tenant
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Tenant {
    pub id: Option<RecordId>,
    pub slug: String,
    pub name: String,
    pub auth0_org_id: Option<String>,
    pub plan: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Default)]
pub struct TenantPatch {
    pub name: Option<String>,
    pub auth0_org_id: Option<String>,
    pub plan: Option<String>,
}

// ─────────────────────────────────────────────
// CP-002: Project
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Project {
    pub id: Option<RecordId>,
    pub tenant: RecordId,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Default)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub repository_url: Option<String>,
}

// ─────────────────────────────────────────────
// CP-003: Stage
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Stage {
    pub id: Option<RecordId>,
    pub project: RecordId,
    pub slug: String,
    pub description: Option<String>,
    pub server: Option<RecordId>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Stage with project info for cross-project queries
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct StageWithProject {
    pub id: Option<RecordId>,
    pub slug: String,
    pub description: Option<String>,
    pub project_slug: String,
    pub project_name: String,
    pub tenant_slug: String,
}

// ─────────────────────────────────────────────
// CP-004: Service
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Service {
    pub id: Option<RecordId>,
    pub stage: RecordId,
    pub slug: String,
    pub image: String,
    pub config: Option<serde_json::Value>,
    pub desired_status: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// ─────────────────────────────────────────────
// CP-005: Container
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Container {
    pub id: Option<RecordId>,
    pub service: RecordId,
    pub container_id: String,
    pub container_name: String,
    pub status: String,
    pub health: Option<String>,
    pub server: Option<RecordId>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}

// ─────────────────────────────────────────────
// CP-006: Server
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Server {
    pub id: Option<RecordId>,
    pub tenant: RecordId,
    pub slug: String,
    pub provider: String,
    pub plan: Option<String>,
    pub ssh_host: String,
    pub ssh_user: String,
    pub deploy_path: String,
    pub status: String,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// ─────────────────────────────────────────────
// CP-007: CostEntry（コスト管理）
// ─────────────────────────────────────────────

/// 月次コストエントリ
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct CostEntry {
    pub id: Option<RecordId>,
    pub tenant: RecordId,
    /// コスト帰属先プロジェクト（None = テナント共通費用）
    pub project: Option<RecordId>,
    /// コスト帰属先ステージ（None = プロジェクト共通）
    pub stage: Option<String>,
    /// プロバイダ種別: sakura, cloudflare, auth0, stripe, other
    pub provider: String,
    /// コスト説明
    pub description: String,
    /// 金額（円）
    pub amount_jpy: i64,
    /// 対象年月（例: "2026-03"）
    pub month: String,
    pub created_at: Option<DateTime<Utc>>,
}

/// 月次コスト集計結果
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct MonthlyCostSummary {
    pub month: String,
    pub provider: String,
    pub project_slug: Option<String>,
    pub total_jpy: i64,
}

// ─────────────────────────────────────────────
// CP-008: DnsRecord（DNS/ドメイン管理）
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct DnsRecord {
    pub id: Option<RecordId>,
    pub tenant: RecordId,
    /// 対象プロジェクト
    pub project: Option<RecordId>,
    /// ドメイン名（例: "api.example.com"）
    pub name: String,
    /// レコードタイプ: A, AAAA, CNAME, TXT 等
    pub record_type: String,
    /// レコード値
    pub content: String,
    /// Cloudflare Zone ID
    pub zone_id: Option<String>,
    /// Cloudflare Record ID
    pub cf_record_id: Option<String>,
    /// プロキシ有効
    pub proxied: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}
