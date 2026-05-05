//! `tenant "<slug>" { ... }` ノードのパース。
//!
//! 文法:
//! ```kdl
//! tenant "<slug>" {
//!     name "..."           // 表示名 (省略時は slug を流用)
//!     auth0_org_id "..."   // Auth0 organization 連携 (省略可)
//!     plan "..."           // billing tier 識別子 (例: plus / pro / team / platform)
//! }
//! ```
//!
//! tenant block は **fleet.kdl 1 ファイルに 1 つだけ** 配置される想定。
//! 同 file 内に複数 tenant block が現れた場合の merge は今は未定義 (parser は last-wins)。

use crate::error::{FlowError, Result};
use crate::model::TenantSpec;
use kdl::KdlNode;

/// `tenant "<slug>" { ... }` ノードを解析して `TenantSpec` を返す。
///
/// slug は entry 1 件目の string 必須。 child block の各 key (name / auth0_org_id /
/// plan) は string 1 件目を抽出。 不明な key は無視 (forward-compat)。
pub fn parse_tenant(node: &KdlNode) -> Result<TenantSpec> {
    let slug = node
        .entries()
        .first()
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| FlowError::InvalidConfig("tenant ノードに slug がありません".into()))?
        .to_string();

    let mut name: Option<String> = None;
    let mut auth0_org_id: Option<String> = None;
    let mut plan: Option<String> = None;

    if let Some(children) = node.children() {
        for child in children.nodes() {
            let key = child.name().value();
            let value = child
                .entries()
                .first()
                .and_then(|e| e.value().as_string())
                .map(|s| s.to_string());
            match key {
                "name" => name = value,
                "auth0_org_id" => auth0_org_id = value,
                "plan" => plan = value,
                _ => {
                    // 不明 key は無視 (forward-compat)
                }
            }
        }
    }

    Ok(TenantSpec {
        slug,
        name,
        auth0_org_id,
        plan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdl::KdlDocument;

    fn first_tenant_node(kdl: &str) -> KdlNode {
        let doc: KdlDocument = kdl.parse().unwrap();
        doc.nodes()
            .iter()
            .find(|n| n.name().value() == "tenant")
            .cloned()
            .expect("tenant ノードが見つかりません")
    }

    #[test]
    fn parse_minimal_slug_only() {
        let node = first_tenant_node(r#"tenant "hq""#);
        let spec = parse_tenant(&node).unwrap();
        assert_eq!(spec.slug, "hq");
        assert_eq!(spec.name, None);
        assert_eq!(spec.auth0_org_id, None);
        assert_eq!(spec.plan, None);
    }

    #[test]
    fn parse_full_block() {
        let node = first_tenant_node(
            r#"tenant "hq" {
                name "FleetStage HQ"
                auth0_org_id "org_abc"
                plan "platform"
            }"#,
        );
        let spec = parse_tenant(&node).unwrap();
        assert_eq!(spec.slug, "hq");
        assert_eq!(spec.name.as_deref(), Some("FleetStage HQ"));
        assert_eq!(spec.auth0_org_id.as_deref(), Some("org_abc"));
        assert_eq!(spec.plan.as_deref(), Some("platform"));
    }

    #[test]
    fn parse_partial_block_only_name() {
        let node = first_tenant_node(
            r#"tenant "anycreative" {
                name "Anycreative Inc"
            }"#,
        );
        let spec = parse_tenant(&node).unwrap();
        assert_eq!(spec.slug, "anycreative");
        assert_eq!(spec.name.as_deref(), Some("Anycreative Inc"));
        assert_eq!(spec.auth0_org_id, None);
        assert_eq!(spec.plan, None);
    }

    #[test]
    fn parse_unknown_key_is_ignored() {
        let node = first_tenant_node(
            r#"tenant "hq" {
                name "FleetStage HQ"
                future_field "something"
            }"#,
        );
        let spec = parse_tenant(&node).unwrap();
        assert_eq!(spec.slug, "hq");
        assert_eq!(spec.name.as_deref(), Some("FleetStage HQ"));
        // future_field は無視される
    }

    #[test]
    fn parse_missing_slug_returns_error() {
        let node = first_tenant_node(r#"tenant"#);
        let result = parse_tenant(&node);
        assert!(result.is_err());
    }
}
