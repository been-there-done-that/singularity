//! Plan Executor — enforces PlanGrant via SQL rewrite.
//!
//! # Contract
//!
//! ```text
//! fn execute(plan, grant, db) -> Result<ExecutionResult, ExecutionError>
//! ```
//!
//! # Preconditions (asserted, not re-checked)
//!
//! - PlanGrant was produced from this LogicalPlan
//! - Capability signature already verified
//! - TTL already validated
//!
//! # What This Module MUST Do
//!
//! - Enforce row predicates
//! - Enforce field masks
//! - Enforce join grants  
//! - Enforce bulk limits
//! - Choose fast path vs slow path
//! - Be SQLite-safe
//!
//! # What This Module MUST NOT Do
//!
//! - Re-authorize anything
//! - Interpret policy rules
//! - "Fix" bad plans
//! - Silently drop forbidden fields
//!
//! If a violation happens → **hard error**.

pub mod error;
pub mod result;
pub mod sql;

pub use error::ExecutionError;
pub use result::{ExecutionResult, Row};

use crate::planner::LogicalPlan;
use crate::protocol::data::{DataAction, PlanGrant, RowPredicate};
use sha2::{Sha256, Digest};

// ============================================================================
// Execution Mode (Observable)
// ============================================================================

/// Execution mode for observability and debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Fast path: pure SQL rewrite, no per-row evaluation.
    Fast,
    /// Slow path: dynamic per-row policy evaluation.
    Slow,
}

impl std::fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fast => write!(f, "FAST"),
            Self::Slow => write!(f, "SLOW"),
        }
    }
}

// ============================================================================
// Plan Hash (Capability Binding)
// ============================================================================

/// Compute a deterministic hash of (LogicalPlan, PlanGrant) for capability binding.
/// 
/// This hash is embedded in the capability payload and verified by the executor
/// to prevent mismatched plan/grant reuse.
pub fn compute_plan_hash(plan: &LogicalPlan, grant: &PlanGrant) -> String {
    let mut hasher = Sha256::new();
    
    // Hash key plan fields (deterministically ordered)
    hasher.update(plan.action.to_string().as_bytes());
    hasher.update(plan.base_model.name.as_bytes());
    hasher.update(plan.base_model.id.as_bytes());
    
    // Hash selection fields
    for field in &plan.selection.base_fields {
        hasher.update(field.name.as_bytes());
    }
    
    // Hash filter (if present)
    if let Some(ref filter) = plan.filter {
        // Serialize filter to JSON for consistent hashing
        if let Ok(json) = serde_json::to_string(filter) {
            hasher.update(json.as_bytes());
        }
    }
    
    // Hash grant fields
    hasher.update(grant.model.as_bytes());
    hasher.update(grant.action.to_string().as_bytes());
    
    // Hash row predicate type
    match &grant.row_predicate {
        RowPredicate::Always => hasher.update(b"ALWAYS"),
        RowPredicate::Never => hasher.update(b"NEVER"),
        RowPredicate::Sql { filter } => {
            hasher.update(b"SQL:");
            if let Ok(json) = serde_json::to_string(filter) {
                hasher.update(json.as_bytes());
            }
        }
        RowPredicate::Dynamic { policy_id, required_fields } => {
            hasher.update(b"DYNAMIC:");
            hasher.update(policy_id.as_bytes());
            for f in required_fields {
                hasher.update(f.as_bytes());
            }
        }
    }
    
    // Return hex-encoded hash (first 16 chars for brevity)
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Verify that a capability's plan hash matches the current plan/grant.
pub fn verify_plan_hash(
    expected_hash: Option<&str>,
    plan: &LogicalPlan,
    grant: &PlanGrant,
) -> Result<(), ExecutionError> {
    if let Some(expected) = expected_hash {
        let actual = compute_plan_hash(plan, grant);
        if actual != expected {
            return Err(ExecutionError::PlanMismatch {
                reason: format!(
                    "plan hash mismatch: expected {}, got {}",
                    expected, actual
                ),
            });
        }
    }
    Ok(())
}

// ============================================================================
// Execute Entry Points
// ============================================================================

/// Execute a plan with the given grant.
///
/// # Fast Path vs Slow Path
///
/// Decision is purely mechanical:
/// - If any row predicate is Dynamic → slow path
/// - Otherwise → fast path (SQL rewrite)
pub fn execute(
    plan: &LogicalPlan,
    grant: &PlanGrant,
    conn: &rusqlite::Connection,
    subject_id: Option<&str>,
) -> Result<ExecutionResult, ExecutionError> {
    let (result, _mode) = execute_with_mode(plan, grant, conn, subject_id)?;
    Ok(result)
}

/// Execute a plan and return the execution mode (for observability).
pub fn execute_with_mode(
    plan: &LogicalPlan,
    grant: &PlanGrant,
    conn: &rusqlite::Connection,
    subject_id: Option<&str>,
) -> Result<(ExecutionResult, ExecutionMode), ExecutionError> {
    // Determine execution path
    let mode = determine_execution_mode(grant);

    let result = match mode {
        ExecutionMode::Fast => execute_fast_path(plan, grant, conn, subject_id)?,
        ExecutionMode::Slow => execute_slow_path(plan, grant, conn, subject_id)?,
    };

    Ok((result, mode))
}

/// Determine execution mode based on grant predicates.
pub fn determine_execution_mode(grant: &PlanGrant) -> ExecutionMode {
    if requires_dynamic_evaluation(grant) {
        ExecutionMode::Slow
    } else {
        ExecutionMode::Fast
    }
}

/// Check if any predicate requires dynamic (per-row) evaluation.
fn requires_dynamic_evaluation(grant: &PlanGrant) -> bool {
    // Check base row predicate
    if grant.row_predicate.is_dynamic() {
        return true;
    }

    // Check join predicates
    for join in &grant.joins {
        if join.row_predicate.is_dynamic() {
            return true;
        }
    }

    false
}

/// Fast path: pure SQL rewrite.
fn execute_fast_path(
    plan: &LogicalPlan,
    grant: &PlanGrant,
    conn: &rusqlite::Connection,
    subject_id: Option<&str>,
) -> Result<ExecutionResult, ExecutionError> {
    match plan.action {
        DataAction::Query => sql::execute_query(plan, grant, conn, subject_id),
        DataAction::Count => sql::execute_count(plan, grant, conn, subject_id),
        DataAction::Insert => sql::execute_insert(plan, grant, conn),
        DataAction::Update => sql::execute_update(plan, grant, conn, subject_id),
        DataAction::Delete => sql::execute_delete(plan, grant, conn, subject_id),
    }
}

/// Slow path: dynamic per-row evaluation.
fn execute_slow_path(
    _plan: &LogicalPlan,
    _grant: &PlanGrant,
    _conn: &rusqlite::Connection,
    _subject_id: Option<&str>,
) -> Result<ExecutionResult, ExecutionError> {
    // For now, return an error - dynamic evaluation is Phase D.2
    // The structure is in place, but we implement fast path first
    
    // Step 1: Execute candidate query (with SQL-compilable predicates only)
    // Step 2: Evaluate dynamic policy per row
    // Step 3: Collect allowed IDs
    // Step 4: Enforce bulk limits
    // Step 5: Re-execute with WHERE id IN (...)
    
    Err(ExecutionError::UnsupportedOperation {
        reason: "dynamic row evaluation not yet implemented".into(),
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::{FieldRef, FieldType, ModelRef, SelectionPlan};
    use crate::protocol::data::{FilterOp, FilterValue, FieldMask, BulkGrant};

    fn test_model() -> ModelRef {
        ModelRef::new("items", "model-items")
    }

    fn test_field(name: &str) -> FieldRef {
        FieldRef::new(test_model(), name, FieldType::String)
    }

    fn test_plan() -> LogicalPlan {
        LogicalPlan {
            action: DataAction::Query,
            base_model: test_model(),
            selection: SelectionPlan::new(vec![test_field("id"), test_field("title")]),
            joins: vec![],
            filter: None,
            mutation: None,
            ordering: vec![],
            pagination: None,
        }
    }

    #[test]
    fn test_requires_dynamic_sql_only() {
        let grant = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Sql {
                filter: FilterOp::Eq("owner_id".into(), FilterValue::Subject),
            },
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };

        assert!(!requires_dynamic_evaluation(&grant));
    }

    #[test]
    fn test_requires_dynamic_always() {
        let grant = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Always,
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };

        assert!(!requires_dynamic_evaluation(&grant));
    }

    #[test]
    fn test_requires_dynamic_true() {
        let grant = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Dynamic {
                policy_id: "custom_policy".into(),
                required_fields: vec!["status".into()],
            },
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };

        assert!(requires_dynamic_evaluation(&grant));
    }

    #[test]
    fn test_execution_mode_display() {
        assert_eq!(ExecutionMode::Fast.to_string(), "FAST");
        assert_eq!(ExecutionMode::Slow.to_string(), "SLOW");
    }

    #[test]
    fn test_determine_execution_mode() {
        let fast_grant = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Always,
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };
        assert_eq!(determine_execution_mode(&fast_grant), ExecutionMode::Fast);

        let slow_grant = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Dynamic {
                policy_id: "policy".into(),
                required_fields: vec![],
            },
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };
        assert_eq!(determine_execution_mode(&slow_grant), ExecutionMode::Slow);
    }

    #[test]
    fn test_compute_plan_hash_deterministic() {
        let plan = test_plan();
        let grant = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Always,
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };

        let hash1 = compute_plan_hash(&plan, &grant);
        let hash2 = compute_plan_hash(&plan, &grant);
        
        assert_eq!(hash1, hash2, "Same input must produce identical hash");
        assert_eq!(hash1.len(), 16, "Hash should be 16 hex chars");
    }

    #[test]
    fn test_compute_plan_hash_differs_by_action() {
        let plan = test_plan();
        
        let grant1 = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Always,
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };
        
        let grant2 = PlanGrant {
            model: "items".into(),
            action: DataAction::Count, // Different action
            row_predicate: RowPredicate::Always,
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };

        let hash1 = compute_plan_hash(&plan, &grant1);
        let hash2 = compute_plan_hash(&plan, &grant2);
        
        assert_ne!(hash1, hash2, "Different actions should produce different hash");
    }

    #[test]
    fn test_verify_plan_hash_success() {
        let plan = test_plan();
        let grant = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Always,
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };

        let hash = compute_plan_hash(&plan, &grant);
        let result = verify_plan_hash(Some(&hash), &plan, &grant);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_plan_hash_mismatch() {
        let plan = test_plan();
        let grant = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Always,
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };

        let result = verify_plan_hash(Some("wronghash1234567"), &plan, &grant);
        assert!(matches!(result, Err(ExecutionError::PlanMismatch { .. })));
    }

    #[test]
    fn test_verify_plan_hash_none_skips_check() {
        let plan = test_plan();
        let grant = PlanGrant {
            model: "items".into(),
            action: DataAction::Query,
            row_predicate: RowPredicate::Always,
            field_mask: FieldMask::default(),
            joins: vec![],
            bulk: BulkGrant::default(),
        };

        let result = verify_plan_hash(None, &plan, &grant);
        assert!(result.is_ok());
    }
}

