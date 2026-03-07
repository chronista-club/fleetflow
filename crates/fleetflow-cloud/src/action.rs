//! Action types for cloud resource management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a planned action for a cloud resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Unique identifier for the action
    pub id: String,

    /// Type of action to perform
    pub action_type: ActionType,

    /// Resource type (e.g., "server", "r2-bucket", "dns-record")
    pub resource_type: String,

    /// Resource identifier
    pub resource_id: String,

    /// Description of the action
    pub description: String,

    /// Additional details about the action
    pub details: HashMap<String, serde_json::Value>,
}

/// Type of action to perform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// Create a new resource
    Create,
    /// Update an existing resource
    Update,
    /// Delete a resource
    Delete,
    /// No changes needed
    NoOp,
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionType::Create => write!(f, "create"),
            ActionType::Update => write!(f, "update"),
            ActionType::Delete => write!(f, "delete"),
            ActionType::NoOp => write!(f, "no-op"),
        }
    }
}

/// Result of applying actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    /// Successfully applied actions
    pub succeeded: Vec<ActionResult>,

    /// Failed actions
    pub failed: Vec<ActionResult>,

    /// Total execution time in milliseconds
    pub duration_ms: u64,
}

impl ApplyResult {
    pub fn new() -> Self {
        Self {
            succeeded: Vec::new(),
            failed: Vec::new(),
            duration_ms: 0,
        }
    }

    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn add_success(&mut self, action_id: String, message: String) {
        self.succeeded.push(ActionResult {
            action_id,
            success: true,
            message,
            error: None,
        });
    }

    pub fn add_failure(&mut self, action_id: String, error: String) {
        self.failed.push(ActionResult {
            action_id,
            success: false,
            message: String::new(),
            error: Some(error),
        });
    }
}

impl Default for ApplyResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a single action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// ID of the action
    pub action_id: String,

    /// Whether the action succeeded
    pub success: bool,

    /// Success message
    pub message: String,

    /// Error message if failed
    pub error: Option<String>,
}

/// Plan containing all actions to be applied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// List of actions to perform
    pub actions: Vec<Action>,

    /// Whether the plan has any changes
    pub has_changes: bool,
}

impl Plan {
    pub fn new(actions: Vec<Action>) -> Self {
        let has_changes = actions.iter().any(|a| a.action_type != ActionType::NoOp);
        Self {
            actions,
            has_changes,
        }
    }

    pub fn empty() -> Self {
        Self {
            actions: Vec::new(),
            has_changes: false,
        }
    }

    /// Get actions by type
    pub fn actions_by_type(&self, action_type: ActionType) -> Vec<&Action> {
        self.actions
            .iter()
            .filter(|a| a.action_type == action_type)
            .collect()
    }

    /// Summary of the plan
    pub fn summary(&self) -> PlanSummary {
        PlanSummary {
            create: self.actions_by_type(ActionType::Create).len(),
            update: self.actions_by_type(ActionType::Update).len(),
            delete: self.actions_by_type(ActionType::Delete).len(),
            no_change: self.actions_by_type(ActionType::NoOp).len(),
        }
    }
}

/// Summary of planned actions
#[derive(Debug, Clone)]
pub struct PlanSummary {
    pub create: usize,
    pub update: usize,
    pub delete: usize,
    pub no_change: usize,
}

impl std::fmt::Display for PlanSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} to create, {} to update, {} to delete, {} unchanged",
            self.create, self.update, self.delete, self.no_change
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ActionType tests ----

    #[test]
    fn test_action_type_display() {
        assert_eq!(ActionType::Create.to_string(), "create");
        assert_eq!(ActionType::Update.to_string(), "update");
        assert_eq!(ActionType::Delete.to_string(), "delete");
        assert_eq!(ActionType::NoOp.to_string(), "no-op");
    }

    #[test]
    fn test_action_type_equality() {
        assert_eq!(ActionType::Create, ActionType::Create);
        assert_ne!(ActionType::Create, ActionType::Update);
    }

    #[test]
    fn test_action_type_serde_roundtrip() {
        let action_type = ActionType::Create;
        let json = serde_json::to_string(&action_type).unwrap();
        assert_eq!(json, "\"create\"");
        let deserialized: ActionType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, action_type);
    }

    #[test]
    fn test_action_type_serde_all_variants() {
        let cases = vec![
            (ActionType::Create, "\"create\""),
            (ActionType::Update, "\"update\""),
            (ActionType::Delete, "\"delete\""),
            (ActionType::NoOp, "\"no_op\""),
        ];
        for (variant, expected_json) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected_json);
            let deserialized: ActionType = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, variant);
        }
    }

    // ---- ApplyResult tests ----

    #[test]
    fn test_apply_result_new_is_success() {
        let result = ApplyResult::new();
        assert!(result.is_success());
        assert!(result.succeeded.is_empty());
        assert!(result.failed.is_empty());
        assert_eq!(result.duration_ms, 0);
    }

    #[test]
    fn test_apply_result_default_equals_new() {
        let default = ApplyResult::default();
        assert!(default.is_success());
        assert!(default.succeeded.is_empty());
        assert!(default.failed.is_empty());
    }

    #[test]
    fn test_apply_result_add_success() {
        let mut result = ApplyResult::new();
        result.add_success("action-1".to_string(), "done".to_string());

        assert!(result.is_success());
        assert_eq!(result.succeeded.len(), 1);
        assert_eq!(result.succeeded[0].action_id, "action-1");
        assert!(result.succeeded[0].success);
        assert_eq!(result.succeeded[0].message, "done");
        assert!(result.succeeded[0].error.is_none());
    }

    #[test]
    fn test_apply_result_add_failure() {
        let mut result = ApplyResult::new();
        result.add_failure("action-2".to_string(), "boom".to_string());

        assert!(!result.is_success());
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].action_id, "action-2");
        assert!(!result.failed[0].success);
        assert_eq!(result.failed[0].error, Some("boom".to_string()));
    }

    #[test]
    fn test_apply_result_mixed() {
        let mut result = ApplyResult::new();
        result.add_success("ok-1".to_string(), "ok".to_string());
        result.add_failure("fail-1".to_string(), "err".to_string());

        assert!(!result.is_success());
        assert_eq!(result.succeeded.len(), 1);
        assert_eq!(result.failed.len(), 1);
    }

    // ---- Plan tests ----

    #[test]
    fn test_plan_empty() {
        let plan = Plan::empty();
        assert!(!plan.has_changes);
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn test_plan_new_with_noop_only() {
        let actions = vec![Action {
            id: "noop-1".to_string(),
            action_type: ActionType::NoOp,
            resource_type: "server".to_string(),
            resource_id: "srv-1".to_string(),
            description: "no change".to_string(),
            details: HashMap::new(),
        }];
        let plan = Plan::new(actions);
        assert!(!plan.has_changes);
    }

    #[test]
    fn test_plan_new_with_changes() {
        let actions = vec![
            Action {
                id: "create-1".to_string(),
                action_type: ActionType::Create,
                resource_type: "server".to_string(),
                resource_id: "srv-1".to_string(),
                description: "create srv-1".to_string(),
                details: HashMap::new(),
            },
            Action {
                id: "noop-2".to_string(),
                action_type: ActionType::NoOp,
                resource_type: "server".to_string(),
                resource_id: "srv-2".to_string(),
                description: "no change".to_string(),
                details: HashMap::new(),
            },
        ];
        let plan = Plan::new(actions);
        assert!(plan.has_changes);
    }

    #[test]
    fn test_plan_actions_by_type() {
        let actions = vec![
            Action {
                id: "create-1".to_string(),
                action_type: ActionType::Create,
                resource_type: "server".to_string(),
                resource_id: "srv-1".to_string(),
                description: "create".to_string(),
                details: HashMap::new(),
            },
            Action {
                id: "delete-1".to_string(),
                action_type: ActionType::Delete,
                resource_type: "server".to_string(),
                resource_id: "srv-2".to_string(),
                description: "delete".to_string(),
                details: HashMap::new(),
            },
            Action {
                id: "create-2".to_string(),
                action_type: ActionType::Create,
                resource_type: "server".to_string(),
                resource_id: "srv-3".to_string(),
                description: "create".to_string(),
                details: HashMap::new(),
            },
        ];
        let plan = Plan::new(actions);

        assert_eq!(plan.actions_by_type(ActionType::Create).len(), 2);
        assert_eq!(plan.actions_by_type(ActionType::Delete).len(), 1);
        assert_eq!(plan.actions_by_type(ActionType::Update).len(), 0);
        assert_eq!(plan.actions_by_type(ActionType::NoOp).len(), 0);
    }

    #[test]
    fn test_plan_summary() {
        let actions = vec![
            Action {
                id: "c1".to_string(),
                action_type: ActionType::Create,
                resource_type: "server".to_string(),
                resource_id: "a".to_string(),
                description: "".to_string(),
                details: HashMap::new(),
            },
            Action {
                id: "c2".to_string(),
                action_type: ActionType::Create,
                resource_type: "server".to_string(),
                resource_id: "b".to_string(),
                description: "".to_string(),
                details: HashMap::new(),
            },
            Action {
                id: "u1".to_string(),
                action_type: ActionType::Update,
                resource_type: "server".to_string(),
                resource_id: "c".to_string(),
                description: "".to_string(),
                details: HashMap::new(),
            },
            Action {
                id: "d1".to_string(),
                action_type: ActionType::Delete,
                resource_type: "server".to_string(),
                resource_id: "d".to_string(),
                description: "".to_string(),
                details: HashMap::new(),
            },
            Action {
                id: "n1".to_string(),
                action_type: ActionType::NoOp,
                resource_type: "server".to_string(),
                resource_id: "e".to_string(),
                description: "".to_string(),
                details: HashMap::new(),
            },
        ];
        let plan = Plan::new(actions);
        let summary = plan.summary();

        assert_eq!(summary.create, 2);
        assert_eq!(summary.update, 1);
        assert_eq!(summary.delete, 1);
        assert_eq!(summary.no_change, 1);
    }

    #[test]
    fn test_plan_summary_display() {
        let summary = PlanSummary {
            create: 3,
            update: 1,
            delete: 2,
            no_change: 5,
        };
        assert_eq!(
            summary.to_string(),
            "3 to create, 1 to update, 2 to delete, 5 unchanged"
        );
    }

    // ---- Action serde test ----

    #[test]
    fn test_action_serde_roundtrip() {
        let action = Action {
            id: "create-web".to_string(),
            action_type: ActionType::Create,
            resource_type: "server".to_string(),
            resource_id: "web-01".to_string(),
            description: "Create web server".to_string(),
            details: [("zone".to_string(), serde_json::json!("tk1a"))]
                .into_iter()
                .collect(),
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "create-web");
        assert_eq!(deserialized.action_type, ActionType::Create);
        assert_eq!(deserialized.resource_type, "server");
        assert_eq!(deserialized.resource_id, "web-01");
        assert_eq!(deserialized.details.get("zone").unwrap(), "tk1a");
    }
}
