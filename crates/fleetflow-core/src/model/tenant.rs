//! Tenant 宣言モデル
//!
//! `tenant "<slug>" { name "..." plan "..." auth0_org_id "..." }` 構文を
//! Flow に attach するための spec。 fleet.kdl 上で project の所有権 (どの tenant の
//! 配下に project が属するか) を **declarative** に記述するための層。
//!
//! deploy 時の tenant_slug 解決優先度 (deploy.rs):
//!   1. CLI flag `--tenant <slug>` (override)
//!   2. fleet.kdl `tenant "<slug>"` block (= TenantSpec.slug)
//!   3. CLI auth context (`creds.tenant_slug`)
//!   4. "default"

use serde::{Deserialize, Serialize};

/// fleet.kdl で宣言された tenant 情報。
///
/// 全 field は controlplane の `tenant` table と 1:1 対応:
/// - `slug` は識別子 (URL safe、 unique)
/// - `name` は表示用、 省略時は slug を流用
/// - `auth0_org_id` は Auth0 organization 連携が必要な tenant のみ設定
/// - `plan` は billing tier (例: `plus` / `pro` / `team` / `platform`、 自由文字列)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantSpec {
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth0_org_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

impl TenantSpec {
    /// slug のみで最小構成 (name / auth0_org_id / plan は controlplane 側 default)。
    pub fn from_slug(slug: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            name: None,
            auth0_org_id: None,
            plan: None,
        }
    }
}
