//! PlanGrant and enforcement artifacts.
//!
//! These structures are produced by policy authorization and consumed by execution.
//! They encode all access constraints in a deterministic, enforceable form.

use serde::{Deserialize, Serialize};

use super::filter::FilterOp;
use super::query::JoinType;

/// Row-level predicate for policy enforcement.
///
/// Determines how row access is controlled during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RowPredicate {
    /// Always allow access (admin, public resources)
    Always,

    /// Always deny access
    Never,

    /// SQL-compilable predicate (injected into WHERE)
    ///
    /// This is the fast path - no per-row evaluation needed.
    Sql {
        /// The filter to inject
        filter: FilterOp,
    },

    /// Requires per-row policy evaluation
    ///
    /// This is the slow path - each candidate row is evaluated against policy.
    Dynamic {
        /// Reference to the policy expression to evaluate
        policy_id: String,
        /// Fields required for policy evaluation (minimizes fetch)
        required_fields: Vec<String>,
    },
}

/// Field-level access masks.
///
/// Determines which fields can be read and written.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FieldMask {
    /// Fields allowed for read/select
    #[serde(default)]
    pub select_allow: Vec<String>,

    /// Fields allowed for write/update
    #[serde(default)]
    pub write_allow: Vec<String>,
}

/// Per-join authorization.
///
/// Each join is independently controlled by policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinGrant {
    /// Relation key (must match __relations)
    pub relation: String,

    /// Allowed join types for this relation
    pub allow_types: Vec<JoinType>,

    /// Fields allowed from joined table
    pub select_allow: Vec<String>,

    /// Row predicate for joined table's rows
    pub row_predicate: RowPredicate,
}

/// Bulk operation constraints.
///
/// Prevents accidental mass operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BulkGrant {
    /// Whether bulk operations are allowed at all
    pub allow: bool,

    /// Require WHERE clause (prevents "DELETE FROM table")
    #[serde(default = "default_true")]
    pub requires_where: bool,

    /// Maximum rows affected per operation
    ///
    /// Enforced in BOTH SQL fast path and dynamic slow path.
    /// If exceeded → 403 Forbidden.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rows: Option<u32>,

    /// Fields allowed in RETURNING clause
    #[serde(default)]
    pub returning_allow: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for BulkGrant {
    fn default() -> Self {
        Self {
            allow: false,
            requires_where: true,
            max_rows: Some(1000), // Safe default
            returning_allow: vec![],
        }
    }
}

/// Complete authorization grant for a planned operation.
///
/// This is what goes into the capability token after policy evaluation.
/// Execution only enforces what's encoded here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanGrant {
    /// Target model
    pub model: String,

    /// Authorized action
    pub action: DataAction,

    /// Row-level predicate (applied to WHERE/ON)
    pub row_predicate: RowPredicate,

    /// Field-level access masks
    pub field_mask: FieldMask,

    /// Per-join grants
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joins: Vec<JoinGrant>,

    /// Bulk operation constraints
    pub bulk: BulkGrant,
}

/// Data plane action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataAction {
    Query,
    Count,
    Insert,
    Update,
    Delete,
}

impl RowPredicate {
    /// Create an ownership predicate (most common case).
    ///
    /// Results in: WHERE owner_id = $subject
    pub fn ownership(owner_field: impl Into<String>) -> Self {
        use super::filter::FilterValue;
        Self::Sql {
            filter: FilterOp::Eq(owner_field.into(), FilterValue::Subject),
        }
    }

    /// Create a dynamic predicate for complex policies.
    pub fn dynamic(policy_id: impl Into<String>, required_fields: Vec<String>) -> Self {
        Self::Dynamic {
            policy_id: policy_id.into(),
            required_fields,
        }
    }

    /// Check if this predicate requires per-row evaluation.
    pub fn is_dynamic(&self) -> bool {
        matches!(self, Self::Dynamic { .. })
    }
}

impl FieldMask {
    /// Create a field mask that allows all fields.
    pub fn all() -> Self {
        Self {
            select_allow: vec!["*".into()],
            write_allow: vec!["*".into()],
        }
    }

    /// Create a read-only field mask.
    pub fn read_only(fields: Vec<String>) -> Self {
        Self {
            select_allow: fields,
            write_allow: vec![],
        }
    }

    /// Check if a field is allowed for reading.
    pub fn can_read(&self, field: &str) -> bool {
        self.select_allow.iter().any(|f| f == "*" || f == field)
    }

    /// Check if a field is allowed for writing.
    pub fn can_write(&self, field: &str) -> bool {
        self.write_allow.iter().any(|f| f == "*" || f == field)
    }
}

impl PlanGrant {
    /// Create a minimal grant for a query operation.
    pub fn query(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Never,
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        }
    }

    /// Set the row predicate.
    pub fn with_row_predicate(mut self, predicate: RowPredicate) -> Self {
        self.row_predicate = predicate;
        self
    }

    /// Set the field mask.
    pub fn with_field_mask(mut self, mask: FieldMask) -> Self {
        self.field_mask = mask;
        self
    }

    /// Add a join grant.
    pub fn with_join(mut self, join: JoinGrant) -> Self {
        self.joins.push(join);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_predicate_ownership() {
        let pred = RowPredicate::ownership("owner_id");
        match pred {
            RowPredicate::Sql { filter: FilterOp::Eq(field, _) } => {
                assert_eq!(field, "owner_id");
            }
            _ => panic!("expected SQL predicate"),
        }
    }

    #[test]
    fn test_row_predicate_dynamic() {
        let pred = RowPredicate::dynamic("policy::custom_access", vec!["id".into(), "status".into()]);
        assert!(pred.is_dynamic());
        
        if let RowPredicate::Dynamic { policy_id, required_fields } = pred {
            assert_eq!(policy_id, "policy::custom_access");
            assert_eq!(required_fields, vec!["id", "status"]);
        }
    }

    #[test]
    fn test_field_mask_checks() {
        let mask = FieldMask {
            select_allow: vec!["id".into(), "name".into()],
            write_allow: vec!["name".into()],
        };
        
        assert!(mask.can_read("id"));
        assert!(mask.can_read("name"));
        assert!(!mask.can_read("secret"));
        
        assert!(!mask.can_write("id"));
        assert!(mask.can_write("name"));
    }

    #[test]
    fn test_field_mask_wildcard() {
        let mask = FieldMask::all();
        assert!(mask.can_read("anything"));
        assert!(mask.can_write("anything"));
    }

    #[test]
    fn test_plan_grant_json_roundtrip() {
        let grant = PlanGrant::query("items")
            .with_row_predicate(RowPredicate::ownership("owner_id"))
            .with_field_mask(FieldMask {
                select_allow: vec!["id".into(), "title".into()],
                write_allow: vec![],
            });

        let json = serde_json::to_string(&grant).unwrap();
        let back: PlanGrant = serde_json::from_str(&json).unwrap();
        assert_eq!(grant, back);
    }

    #[test]
    fn test_bulk_grant_defaults() {
        let bulk = BulkGrant::default();
        assert!(!bulk.allow);
        assert!(bulk.requires_where);
        assert_eq!(bulk.max_rows, Some(1000));
    }
}
