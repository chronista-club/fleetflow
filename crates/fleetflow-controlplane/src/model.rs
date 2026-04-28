use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

/// Label map — `{"tier": "pro", "region": "tokyo"}` 形式のキー値バッグ (FSC-26)
///
/// `serde_json::Value` で受ける手もあるが、SurrealDB bind 経由で
/// Connection uninitialised が起きる事象を避けるため、素直な `BTreeMap` を使う。
pub type LabelMap = BTreeMap<String, String>;

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
    /// Placement Policy — Scheduler 配置決定時の制約 (FSC-26 Phase B-3)
    pub placement_policy: Option<PlacementPolicy>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Resource quota — tier に応じたテナント上限 (FSC-26 Phase B-3)
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct ResourceQuota {
    pub max_stages: Option<i64>,
    pub max_services_per_stage: Option<i64>,
    pub cpu_cores: Option<i64>,
    pub memory_gb: Option<i64>,
}

/// Fallback chain: required 一致 pool 枯渇時の緩和順序 (FSC-26 Phase B-3)
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct FallbackPolicy {
    /// 緩和順序 (例: ["class", "region"])
    pub relax_order: Option<Vec<String>>,
    /// 最大緩和ホップ数
    pub max_hops: Option<i64>,
}

/// Spread constraint: 配置分散保証 (k8s PodTopologySpread 相当、FSC-26 Phase B-3)
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct SpreadConstraint {
    /// 分散 dimension (例: "region" / "class")
    pub topology_key: Option<String>,
    /// 許容する skew の最大値
    pub max_skew: Option<i64>,
}

/// Placement strategy の文字列定数 (FSC-26 Phase B-3)
///
/// Scheduler がどの方針で pool 内 worker を選ぶかを指定する。
pub mod placement_strategy {
    /// Pool 内で広く分散配置（デフォルト、可用性重視）
    pub const SPREAD_ACROSS_POOL: &str = "spread_across_pool";
    /// 専有 Pool に詰め込み（enterprise isolated tenant 向け）
    pub const PACK_INTO_DEDICATED: &str = "pack_into_dedicated";
    /// 使用率の低い worker から埋める（コスト効率重視）
    pub const FILL_LOWEST: &str = "fill_lowest";
}

/// Tenant の Placement Policy (FSC-26 Phase B-3)
///
/// Scheduler が配置決定時に参照。`tier` は tenant の SSOT（`required_labels.tier`
/// には展開しない）、他は fallback / spread / strategy などの制御パラメータ。
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct PlacementPolicy {
    /// "free" / "pro" / "enterprise" 等（tenant の SSOT）
    pub tier: Option<String>,
    /// tier 以外の soft preference
    pub preferred_labels: Option<LabelMap>,
    /// tier 上限
    pub resource_quota: Option<ResourceQuota>,
    /// Fallback chain
    pub fallback_policy: Option<FallbackPolicy>,
    /// Spread 制約
    pub spread_constraint: Option<SpreadConstraint>,
    /// Placement 戦略 (`placement_strategy` モジュールの定数)
    pub strategy: Option<String>,
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
// Stage Tier adopt DTO (FSC-16, 2026-04-24)
//
// 既存稼働中の stage を非破壊で fleetstage registry に登録するための
// request/outcome 型。Persistence Volume Tier の BYO adopt と同系。
// ─────────────────────────────────────────────

/// adopt_stage に渡す service 1 件の最小 spec。
/// 現時点では slug / image のみを記録し、config は None で始める。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptServiceSpec {
    pub slug: String,
    pub image: String,
}

/// `Database::adopt_stage` への入力。borrowed form のまとまり。
///
/// `clippy::too_many_arguments` を避けつつ、project/stage の必須フィールドと
/// optional (project_name, description) を 1 つの struct に集約する。
#[derive(Debug)]
pub struct AdoptStageRequest<'a> {
    pub tenant_id: &'a RecordId,
    pub server_id: &'a RecordId,
    pub project_slug: &'a str,
    pub project_name: Option<&'a str>,
    pub stage_slug: &'a str,
    pub description: Option<&'a str>,
    pub services: &'a [AdoptServiceSpec],
}

/// adopt_stage の結果: どの project / stage / services が使われたか。
/// project は既存なら再利用、stage と services は常に新規作成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptStageOutcome {
    pub project: Project,
    pub stage: Stage,
    pub services: Vec<Service>,
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

/// Server desired state の文字列定数 (single-table lifecycle model)
/// user の意図を表す。物理 `status` (observed) とは別。
pub mod desired_state {
    /// 通常稼働
    pub const RUNNING: &str = "running";
    /// 計画 maintenance / replace 前準備
    pub const CORDONED: &str = "cordoned";
    /// 廃止予定 / 廃止済み（record は残す、`status="decommissioned"` で確定）
    pub const DECOMMISSIONED: &str = "decommissioned";
}

/// Sakura Cloud infrastructure metadata
/// VM spawn 時に provisioning controller が書き込む
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct SakuraInfo {
    pub server_id: Option<i64>,
    pub disk_id: Option<i64>,
    pub archive_id: Option<i64>,
    pub zone: Option<String>,
}

/// Tailscale tailnet metadata
/// `tailscale up --authkey` 完了時に書き込む
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct TailscaleInfo {
    pub hostname: Option<String>,
    pub tailnet_ip: Option<String>,
    pub node_id: Option<String>,
    pub joined_at: Option<DateTime<Utc>>,
}

/// DNS / public network metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct DnsInfo {
    pub fqdn: Option<String>,
    pub cloudflare_record_id: Option<String>,
    pub public_ipv4: Option<String>,
    pub public_ipv6: Option<String>,
}

/// Lifecycle audit metadata (set once, immutable)
/// replace は新 record + 旧 record の `replaced_from` link で表現
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct ServerLifecycle {
    pub spawned_at: Option<DateTime<Utc>>,
    pub last_replaced_at: Option<DateTime<Utc>>,
    pub decommissioned_at: Option<DateTime<Utc>>,
    pub replaced_from: Option<RecordId>,
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
    /// 所属 Worker Pool (FSC-26 Phase B-2)
    /// None = 未割当（通常は migration で `worker_pool:default` が入る）
    pub pool_id: Option<RecordId>,
    /// User-declared desired state ("running" | "cordoned" | "decommissioned")
    /// `desired_state` モジュールの定数を使用
    pub desired_state: Option<String>,
    /// Human-readable purpose / role description
    pub purpose: Option<String>,
    /// Owner (e.g., user email or team)
    pub owner: Option<String>,
    /// Sakura cloud infrastructure metadata
    pub sakura: Option<SakuraInfo>,
    /// Tailscale tailnet metadata
    pub tailscale: Option<TailscaleInfo>,
    /// DNS / public network metadata
    pub dns: Option<DnsInfo>,
    /// Lifecycle audit (immutable once set)
    pub lifecycle: Option<ServerLifecycle>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// ─────────────────────────────────────────────
// CP-006b: Worker Pool (FSC-26 Phase B-2)
// ─────────────────────────────────────────────

/// Worker Pool — 複数 Server を label で束ねた論理グループ (FSC-26 Phase B-2)
///
/// Placement 決定時に tenant は `required_labels` が全一致する pool を選び、
/// pool 内の worker (server) から `preferred_labels` でランキングする。
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct WorkerPool {
    pub id: Option<RecordId>,
    /// Pool 識別名 (例: "sakura-pro-tokyo" / "default")
    pub name: String,
    pub description: Option<String>,
    /// Hard constraint: pool 所属条件（全ラベル一致必須）
    pub required_labels: Option<LabelMap>,
    /// Soft hint: pool 内での worker ranking に使う
    pub preferred_labels: Option<LabelMap>,
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

// ─────────────────────────────────────────────
// CP-010: Volume (Persistence Volume Tier P-1)
//
// fleetstage の tenant 永続データを格納する disk object の抽象。
// Compute (container) と独立した lifecycle を持つ、Storage 側 SSOT。
// 詳細設計: fleetstage repo docs/design/20-persistence-volume-tier.md
// ─────────────────────────────────────────────

/// Disk Tier の string 定数 (5 段階、D0〜D4)。
///
/// CLI / API / KDL で文字列値として流通するため、const で統一。
pub mod volume_tier {
    /// D0: container scratch、container 寿命のみ
    pub const EPHEMERAL: &str = "ephemeral";
    /// D1: VPS 内 local disk、VPS 寿命と一致、Creo 既存を BYO で受け入れる tier
    pub const LOCAL_VOLUME: &str = "local-volume";
    /// D2: Sakura ディスク等の独立 disk オブジェクト、detach/attach 可
    pub const ATTACHED_DISK: &str = "attached-disk";
    /// D3: object storage が SSOT、disk は cache (v2 future)
    pub const OBJECT_BACKED: &str = "object-backed";
    /// D4: SurrealDB Cloud 等 managed service
    pub const MANAGED_CLOUD: &str = "managed-cloud";

    pub const ALL: &[&str] = &[
        EPHEMERAL,
        LOCAL_VOLUME,
        ATTACHED_DISK,
        OBJECT_BACKED,
        MANAGED_CLOUD,
    ];

    pub fn is_valid(s: &str) -> bool {
        ALL.contains(&s)
    }
}

/// Volume の lifecycle 状態。
pub mod volume_state {
    /// 作成中 (attached-disk 等の provisioning 途中)
    pub const PROVISIONING: &str = "provisioning";
    /// server に attach 済で使用中
    pub const ATTACHED: &str = "attached";
    /// 既存 attach を解除、再 attach 可
    pub const DETACHED: &str = "detached";
    /// 退役、但し削除禁止原則により物理的には保持
    pub const ARCHIVED: &str = "archived";
    /// tier migration 中
    pub const MIGRATING: &str = "migrating";
    /// provision / attach 失敗状態
    pub const FAILED: &str = "failed";

    pub const ALL: &[&str] = &[
        PROVISIONING,
        ATTACHED,
        DETACHED,
        ARCHIVED,
        MIGRATING,
        FAILED,
    ];

    pub fn is_valid(s: &str) -> bool {
        ALL.contains(&s)
    }
}

/// Backup policy (tenant or volume level)
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct VolumeBackupPolicy {
    /// cron expression (UTC), None = inherit from tenant default
    pub schedule: Option<String>,
    /// `surreal export` 等の logical backup を取るか
    pub logical: Option<bool>,
    /// disk snapshot 等の physical snapshot を取るか
    pub physical_snapshot: Option<bool>,
    /// backup 保持日数
    pub retention_days: Option<i64>,
    /// off-site backup の格納先 (例: "s3://fleetstage-backups/tenant/")
    pub destination: Option<String>,
}

/// Persistent Volume — tenant の永続データを保持する disk 抽象。
///
/// 1 volume = 1 disk object (D2) / 1 mount path (D1) / 1 bucket (D3) / 1 managed DB (D4)。
/// Compute (Current) の lifecycle と独立し、detach/attach や tier migration を可能にする。
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct Volume {
    pub id: Option<RecordId>,
    pub tenant: RecordId,
    /// 所属 project (optional: tenant 全体共有の volume もありうる)
    pub project: Option<RecordId>,
    /// 所属 stage (optional)
    pub stage: Option<RecordId>,
    pub slug: String,
    /// Disk Tier: `volume_tier` モジュールの定数を使用
    pub tier: String,
    /// 容量 (bytes)、BYO の場合は unknown = None
    pub size_bytes: Option<i64>,
    /// container にマウントされる path (例: "/var/lib/surrealdb")
    pub mount: String,
    /// 現在 attach されている server、detached の場合は None
    pub server: Option<RecordId>,
    /// プロバイダ名 (例: "sakura-cloud", "local", "s3", "surrealdb-cloud")
    pub provider: String,
    /// provider 側のリソース ID (disk ID, bucket ARN 等)、BYO の場合は None
    pub provider_resource_id: Option<String>,
    /// at-rest 暗号化
    pub encryption: bool,
    /// バックアップ policy (tenant default を継承可)
    pub backup_policy: Option<VolumeBackupPolicy>,
    /// tenant 所有 disk を fleetstage registry に adopt した BYO か
    pub bring_your_own: bool,
    /// 現在の lifecycle 状態: `volume_state` モジュールの定数
    pub state: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Volume snapshot の種別
pub mod volume_snapshot_kind {
    /// Sakura disk snapshot 等、provider 側 atomic snapshot
    pub const DISK_SNAPSHOT: &str = "disk-snapshot";
    /// `surreal export` 等 logical backup
    pub const SURREAL_EXPORT: &str = "surreal-export";
    /// rsync + tar archive の off-site copy
    pub const RSYNC_TAR: &str = "rsync-tar";

    pub const ALL: &[&str] = &[DISK_SNAPSHOT, SURREAL_EXPORT, RSYNC_TAR];

    pub fn is_valid(s: &str) -> bool {
        ALL.contains(&s)
    }
}

/// Volume の snapshot / backup 記録。
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct VolumeSnapshot {
    pub id: Option<RecordId>,
    pub volume: RecordId,
    /// snapshot 種別: `volume_snapshot_kind` モジュールの定数
    pub kind: String,
    /// provider 側の snapshot / archive ID
    pub provider_resource_id: Option<String>,
    /// off-site 格納先 URL (例: "s3://.../backup.surql.gz")
    pub location_url: Option<String>,
    /// snapshot サイズ (bytes)、取得時不明なら None
    pub size_bytes: Option<i64>,
    pub taken_at: Option<DateTime<Utc>>,
    /// 保持期限、越えたら cleanup 対象
    pub retention_until: Option<DateTime<Utc>>,
}

// ─────────────────────────────────────────────
// CP-011: BuildJob (Build Tier v1 MVP)
//
// tenant の build ジョブを記録する。Build Tier B1 (ephemeral-vps-shared)。
// 詳細設計: fleetstage repo docs/design/30-build-tier.md
// ─────────────────────────────────────────────

/// Build ジョブの lifecycle 状態。
pub mod build_job_state {
    /// キューに積まれた (pending)
    pub const QUEUED: &str = "queued";
    /// Build VPS に割り当て済み
    pub const ASSIGNED: &str = "assigned";
    /// git clone 中
    pub const CLONING: &str = "cloning";
    /// docker build 実行中
    pub const BUILDING: &str = "building";
    /// registry push 中
    pub const PUSHING: &str = "pushing";
    /// 成功完了
    pub const SUCCESS: &str = "success";
    /// 失敗
    pub const FAILED: &str = "failed";
    /// キャンセル
    pub const CANCELLED: &str = "cancelled";

    pub const ALL: &[&str] = &[
        QUEUED, ASSIGNED, CLONING, BUILDING, PUSHING, SUCCESS, FAILED, CANCELLED,
    ];

    pub fn is_valid(s: &str) -> bool {
        ALL.contains(&s)
    }
}

/// Build ジョブの種別。
pub mod build_job_kind {
    /// Docker イメージ build
    pub const DOCKER_IMAGE: &str = "docker-image";
    /// Cargo バイナリ build
    pub const CARGO_BINARY: &str = "cargo-binary";
    /// 静的サイト build
    pub const STATIC_SITE: &str = "static-site";

    pub const ALL: &[&str] = &[DOCKER_IMAGE, CARGO_BINARY, STATIC_SITE];

    pub fn is_valid(s: &str) -> bool {
        ALL.contains(&s)
    }
}

/// Build のソース (git リポジトリ + dockerfile パス)
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct BuildSource {
    pub git_url: String,
    /// ブランチ / タグ / SHA。スキーマフィールド名 "git_ref" でそのまま格納。
    pub git_ref: String,
    pub dockerfile: Option<String>,
}

/// Build のターゲット (イメージタグ + registry 認証情報)
#[derive(Debug, Clone, Default, Serialize, Deserialize, SurrealValue)]
pub struct BuildTarget {
    pub image: Option<String>,
    pub registry_secret: Option<String>,
}

/// Build ジョブ — tenant の build リクエストを記録する。
///
/// 1 job = 1 docker build 実行 (またはそれに相当する操作)。
/// fleet-agent の "build" コマンドで実行される。
#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct BuildJob {
    pub id: Option<RecordId>,
    pub tenant: RecordId,
    /// 所属 project (optional)
    pub project: Option<RecordId>,
    /// Build 種別: `build_job_kind` モジュールの定数
    pub kind: String,
    /// ソース情報 (git repository)
    pub source: BuildSource,
    /// ターゲット情報 (image tag 等)
    pub target: BuildTarget,
    /// 現在の lifecycle 状態: `build_job_state` モジュールの定数
    pub state: String,
    /// 割り当てられた Build VPS の server ID
    pub server: Option<RecordId>,
    /// ログの参照先 URL (v1 は polling、logs_url を polling する)
    pub logs_url: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    /// 実行時間 (秒)
    pub duration_seconds: Option<i64>,
}
