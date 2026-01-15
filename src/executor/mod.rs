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
    // Determine execution path
    let requires_dynamic = requires_dynamic_evaluation(grant);

    if requires_dynamic {
        execute_slow_path(plan, grant, conn, subject_id)
    } else {
        execute_fast_path(plan, grant, conn, subject_id)
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
    plan: &LogicalPlan,
    grant: &PlanGrant,
    conn: &rusqlite::Connection,
    subject_id: Option<&str>,
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
    use crate::protocol::data::{FilterOp, FilterValue, FieldMask, BulkGrant};

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
}
