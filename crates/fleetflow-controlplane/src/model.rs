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
    /// DNS プロバイダー（e.g., "cloudflare"）
    pub dns_provider: Option<String>,
    /// テナントのドメイン（e.g., "anycreative.tech"）
    pub dns_domain: Option<String>,
    /// Cloudflare Zone ID
    pub dns_zone_id: Option<String>,
    /// 暗号化された API トークン（AES-256-GCM, base64）
    pub dns_api_token_encrypted: Option<String>,
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
// CP-001b: TenantUser（テナント所属ユーザー）
// ─────────────────────────────────────────────

/// テナントユーザーの役割
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TenantRole {
    /// テナント全権。1テナントに1人。削除不可
    Owner,
    /// ユーザー管理 + 全リソース操作
    Admin,
    /// リソース閲覧 + 操作（ユーザー管理不可）
    Member,
}

impl std::fmt::Display for TenantRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Owner => write!(f, "owner"),
            Self::Admin => write!(f, "admin"),
            Self::Member => write!(f, "member"),
        }
    }
}

impl std::str::FromStr for TenantRole {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            _ => Err(anyhow::anyhow!("不明な role: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct TenantUser {
    pub id: Option<RecordId>,
    /// Auth0 user ID (e.g., "auth0|xxx")
    pub auth0_sub: String,
    /// テナント参照
    pub tenant: RecordId,
    /// 役割: owner / admin / member
    pub role: String,
    pub created_at: Option<DateTime<Utc>>,
}

/// テナント解決結果（auth middleware → handler 受け渡し用）
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Auth0 user ID
    pub sub: String,
    /// Email (from JWT)
    pub email: Option<String>,
    /// テナント slug
    pub tenant_slug: String,
    /// ユーザーの役割
    pub role: TenantRole,
}

impl AuthContext {
    /// owner or admin かどうか
    pub fn can_manage_users(&self) -> bool {
        matches!(self.role, TenantRole::Owner | TenantRole::Admin)
    }
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
    /// プロビジョニングバージョン（e.g., "v2"）
    pub provision_version: Option<String>,
    /// ツールバージョン情報（JSON: {"docker": "29.3.0", "tailscale": "1.94.2", ...}）
    pub tool_versions: Option<serde_json::Value>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// サーバーステータス一括更新用
#[derive(Debug, Clone)]
pub struct ServerStatusUpdate {
    pub slug: String,
    pub status: String,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
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

// ─────────────────────────────────────────────
// CP-009: Deployment（デプロイ履歴）
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Deployment {
    pub id: Option<RecordId>,
    pub tenant: RecordId,
    pub project: RecordId,
    pub stage: String,
    pub server_slug: String,
    /// デプロイの状態: pending, running, success, failed, rolled_back
    pub status: String,
    /// 実行したコマンドまたは playbook 名
    pub command: String,
    /// 実行ログ
    pub log: Option<String>,
    /// デプロイ開始時刻
    pub started_at: Option<DateTime<Utc>>,
    /// デプロイ終了時刻
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}
