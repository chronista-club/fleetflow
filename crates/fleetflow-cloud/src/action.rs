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
