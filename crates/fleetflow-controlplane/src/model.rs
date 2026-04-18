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
    /// 役割: owner / admin / member（DB 上は String）
    pub role: String,
    pub created_at: Option<DateTime<Utc>>,
}

impl TenantUser {
    /// role を TenantRole enum として取得
    pub fn tenant_role(&self) -> TenantRole {
        self.role.parse().unwrap_or(TenantRole::Member)
    }
}

// ─────────────────────────────────────────────
// CP-010: Alert（コンテナアラート）
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Alert {
    pub id: Option<RecordId>,
    pub tenant: RecordId,
    pub server_slug: String,
    pub container_name: String,
    /// "restart_loop" | "unexpected_stop" | "unhealthy"
    pub alert_type: String,
    /// "warning" | "critical"
    pub severity: String,
    pub message: String,
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
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
    /// ユーザー管理操作が可能か（owner/admin）
    pub fn can_manage_users(&self) -> bool {
        matches!(self.role, TenantRole::Owner | TenantRole::Admin)
    }

    /// インフラ操作が可能か（再デプロイ・再起動・ヘルスチェック・DNS 同期等）
    /// 現在は can_manage_users() と同じ判定だが、将来 Member にインフラ操作を
    /// 開放する場合はここだけ変更する（ユーザー管理は Owner/Admin のまま）。
    pub fn can_operate(&self) -> bool {
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

/// 1st ビュー用: ステージ概要（サーバー・デプロイ情報付き）
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct StageOverview {
    pub id: Option<RecordId>,
    pub slug: String,
    pub description: Option<String>,
    pub project_slug: String,
    pub project_name: String,
    /// サーバー slug（紐付いている場合）
    pub server_slug: Option<String>,
    /// サーバーステータス: online / offline / unknown
    pub server_status: Option<String>,
    /// サーバー最終ハートビート
    pub server_heartbeat: Option<DateTime<Utc>>,
    /// 直近デプロイのステータス: success / failed / running / pending
    pub last_deploy_status: Option<String>,
    /// 直近デプロイの時刻
    pub last_deploy_at: Option<DateTime<Utc>>,
    /// アクティブアラート数
    pub alert_count: Option<i64>,
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

/// Server labels — Worker Pool membership 判定と Placement 決定に使う
/// (FSC-26 Phase B-1)
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct ServerLabels {
    /// Tier label: "free" / "pro" / "enterprise" 等
    pub tier: Option<String>,
    /// Region label: "tokyo" / "osaka" 等
    pub region: Option<String>,
    /// Isolation class: "shared" / "dedicated" / "isolated" / "byoc"
    pub class: Option<String>,
    /// CPU architecture: "amd64" / "arm64"
    pub arch: Option<String>,
    /// 任意拡張ラベル
    pub extras: Option<serde_json::Value>,
}

/// Server capacity — 物理リソース上限 (FSC-26 Phase B-1)
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct ServerCapacity {
    pub cpu_cores: Option<i64>,
    pub memory_gb: Option<i64>,
    pub disk_gb: Option<i64>,
}

/// Server allocated — 現在使用中のリソース (FSC-26 Phase B-1)
/// 2-phase placement の commit 時に加算され、release 時に減算される
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct ServerAllocated {
    pub cpu_cores: Option<i64>,
    pub memory_gb: Option<i64>,
}

/// Server scheduling state の文字列定数 (FSC-26 Phase B-1)
/// 論理的なスケジューリング可否を表す。物理 `status` フィールドとは直交:
/// - 物理 `status`: "online" / "offline" / ... (Tailscale / heartbeat の実態)
/// - 論理 `scheduling`: "schedulable" / "cordon" / "drain" (Scheduler の判断)
///
/// k8s Node Condition + `kubectl cordon` / `kubectl drain` 相当。
pub mod scheduling_state {
    /// 新規配置可能、既存 placement も継続
    pub const SCHEDULABLE: &str = "schedulable";
    /// 新規配置停止、既存 placement は継続（計画 maintenance 前の準備）
    pub const CORDON: &str = "cordon";
    /// 既存 placement も退去予定、新規配置不可（廃止 or 大規模 maintenance）
    pub const DRAIN: &str = "drain";
}

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
    /// Worker Pool membership 判定用 labels (FSC-26 Phase B-1)
    pub labels: Option<ServerLabels>,
    /// 物理リソース上限 (FSC-26 Phase B-1)
    pub capacity: Option<ServerCapacity>,
    /// 現在使用中のリソース (FSC-26 Phase B-1)
    pub allocated: Option<ServerAllocated>,
    /// 論理スケジューリング状態 (FSC-26 Phase B-1)
    /// `scheduling_state` モジュールの定数を使用。None = 未指定（schedulable 扱い）
    pub scheduling: Option<String>,
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
