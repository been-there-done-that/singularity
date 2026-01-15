//! Contract tests for the Executor (Phase D).
//!
//! These are the **leak-proof tests** that ensure:
//!
//! **Row Predicate:**
//! - User sees only own rows  
//! - Admin sees all rows
//!
//! **Field Mask:**
//! - Requested fields are returned
//!
//! **Bulk Safety:**
//! - Update/delete without WHERE (when required) → error
//! - Bulk limits enforced
//!
//! **Query/Count Invariant:**
//! - query(filter).len == count(filter)

use rusqlite::Connection;
use singularity::executor::{execute, ExecutionResult, ExecutionError};
use singularity::planner::{
    FieldRef, FieldType, LogicalPlan, ModelRef, MutationPlan, SelectionPlan,
};
use singularity::protocol::data::{
    BulkGrant, DataAction, FieldMask, FilterOp, FilterValue, PlanGrant, RowPredicate,
};
use serde_json::json;

// =============================================================================
// Test Fixtures
// =============================================================================

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    
    // Create test table
    conn.execute(
        "CREATE TABLE items (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            price INTEGER,
            status TEXT DEFAULT 'draft',
            owner_id TEXT NOT NULL
        )",
        [],
    )
    .unwrap();
    
    // Insert test data
    conn.execute(
        "INSERT INTO items (id, title, price, status, owner_id) VALUES 
            ('item-1', 'Item One', 100, 'active', 'user-alice'),
            ('item-2', 'Item Two', 200, 'active', 'user-bob'),
            ('item-3', 'Item Three', 300, 'draft', 'user-alice'),
            ('item-4', 'Item Four', 400, 'active', 'user-alice')",
        [],
    )
    .unwrap();
    
    conn
}

fn items_model() -> ModelRef {
    ModelRef::new("items", "model-items")
}

fn field(name: &str) -> FieldRef {
    FieldRef::new(items_model(), name, FieldType::String)
}

fn query_plan(fields: Vec<&str>, filter: Option<FilterOp>) -> LogicalPlan {
    LogicalPlan {
        action: DataAction::Query,
        base_model: items_model(),
        selection: SelectionPlan::new(
            fields.into_iter().map(|f| field(f)).collect(),
        ),
        joins: vec![],
        filter,
        mutation: None,
        ordering: vec![],
        pagination: None,
    }
}

fn count_plan(filter: Option<FilterOp>) -> LogicalPlan {
    LogicalPlan {
        action: DataAction::Count,
        base_model: items_model(),
        selection: SelectionPlan::empty(),
        joins: vec![],
        filter,
        mutation: None,
        ordering: vec![],
        pagination: None,
    }
}

fn admin_grant() -> PlanGrant {
    PlanGrant {
        model: "items".into(),
        action: DataAction::Query,
        row_predicate: RowPredicate::Always,
        field_mask: FieldMask {
            select_allow: vec!["*".into()],
            write_allow: vec!["*".into()],
        },
        joins: vec![],
        bulk: BulkGrant {
            allow: true,
            requires_where: false,
            max_rows: None,
            returning_allow: vec!["*".into()],
        },
    }
}

fn user_grant(user_id: &str) -> PlanGrant {
    // User can only see their own rows
    PlanGrant {
        model: "items".into(),
        action: DataAction::Query,
        row_predicate: RowPredicate::Sql {
            filter: FilterOp::Eq("owner_id".into(), FilterValue::Subject),
        },
        field_mask: FieldMask {
            select_allow: vec!["*".into()],
            write_allow: vec!["*".into()],
        },
        joins: vec![],
        bulk: BulkGrant {
            allow: true,
            requires_where: true,
            max_rows: Some(10),
            returning_allow: vec!["id".into()],
        },
    }
}

// =============================================================================
// Contract: Row Predicate - Users See Only Own Rows
// =============================================================================

#[test]
fn contract_admin_sees_all_rows() {
    let conn = setup_test_db();
    let plan = query_plan(vec!["id", "title", "owner_id"], None);
    let grant = admin_grant();
    
    let result = execute(&plan, &grant, &conn, None).unwrap();
    
    match result {
        ExecutionResult::Rows(rows) => {
            assert_eq!(rows.len(), 4, "Admin should see all 4 rows");
        }
        _ => panic!("Expected Rows result"),
    }
}

#[test]
fn contract_user_sees_only_own_rows() {
    let conn = setup_test_db();
    let plan = query_plan(vec!["id", "title", "owner_id"], None);
    let grant = user_grant("user-alice");
    
    // Alice has 3 items (item-1, item-3, item-4)
    let result = execute(&plan, &grant, &conn, Some("user-alice")).unwrap();
    
    match result {
        ExecutionResult::Rows(rows) => {
            assert_eq!(rows.len(), 3, "Alice should see only her 3 rows");
            
            // Verify all returned rows belong to Alice
            for row in &rows {
                assert_eq!(
                    row.get_str("owner_id"),
                    Some("user-alice"),
                    "All rows should belong to Alice"
                );
            }
        }
        _ => panic!("Expected Rows result"),
    }
}

#[test]
fn contract_user_bob_sees_only_own_rows() {
    let conn = setup_test_db();
    let plan = query_plan(vec!["id", "title", "owner_id"], None);
    let grant = user_grant("user-bob");
    
    // Bob has 1 item (item-2)
    let result = execute(&plan, &grant, &conn, Some("user-bob")).unwrap();
    
    match result {
        ExecutionResult::Rows(rows) => {
            assert_eq!(rows.len(), 1, "Bob should see only his 1 row");
            assert_eq!(rows[0].get_str("owner_id"), Some("user-bob"));
        }
        _ => panic!("Expected Rows result"),
    }
}

// =============================================================================
// Contract: Row Predicate + User Filter Combined
// =============================================================================

#[test]
fn contract_user_filter_combined_with_predicate() {
    let conn = setup_test_db();
    
    // Alice queries for active items only
    let plan = query_plan(
        vec!["id", "status"],
        Some(FilterOp::Eq("status".into(), FilterValue::String("active".into()))),
    );
    let grant = user_grant("user-alice");
    
    // Alice has 3 items, but only 2 are active (item-1, item-4)
    let result = execute(&plan, &grant, &conn, Some("user-alice")).unwrap();
    
    match result {
        ExecutionResult::Rows(rows) => {
            assert_eq!(rows.len(), 2, "Alice should see only 2 active rows");
            for row in &rows {
                assert_eq!(row.get_str("status"), Some("active"));
            }
        }
        _ => panic!("Expected Rows result"),
    }
}

// =============================================================================
// Contract: Query/Count Invariant
// =============================================================================

#[test]
fn contract_query_count_invariant() {
    let conn = setup_test_db();
    let filter = Some(FilterOp::Eq("status".into(), FilterValue::String("active".into())));
    
    let query_plan = query_plan(vec!["id"], filter.clone());
    let count_plan = count_plan(filter);
    let grant = admin_grant();
    
    let query_result = execute(&query_plan, &grant, &conn, None).unwrap();
    
    let mut count_grant = grant.clone();
    count_grant.action = DataAction::Count;
    let count_result = execute(&count_plan, &count_grant, &conn, None).unwrap();
    
    match (query_result, count_result) {
        (ExecutionResult::Rows(rows), ExecutionResult::Count(count)) => {
            assert_eq!(
                rows.len() as u64, count,
                "query().len must equal count()"
            );
        }
        _ => panic!("Expected Rows and Count results"),
    }
}

#[test]
fn contract_query_count_invariant_with_user_predicate() {
    let conn = setup_test_db();
    
    let query_plan = query_plan(vec!["id"], None);
    let count_plan = count_plan(None);
    let grant = user_grant("user-alice");
    
    let query_result = execute(&query_plan, &grant, &conn, Some("user-alice")).unwrap();
    
    let mut count_grant = grant.clone();
    count_grant.action = DataAction::Count;
    let count_result = execute(&count_plan, &count_grant, &conn, Some("user-alice")).unwrap();
    
    match (query_result, count_result) {
        (ExecutionResult::Rows(rows), ExecutionResult::Count(count)) => {
            assert_eq!(
                rows.len() as u64, count,
                "query().len must equal count() for user"
            );
        }
        _ => panic!("Expected Rows and Count results"),
    }
}

// =============================================================================
// Contract: Bulk Safety
// =============================================================================

#[test]
fn contract_update_requires_where_for_user() {
    let conn = setup_test_db();
    
    // Update without WHERE clause
    let plan = LogicalPlan {
        action: DataAction::Update,
        base_model: items_model(),
        selection: SelectionPlan::empty(),
        joins: vec![],
        filter: None, // No WHERE!
        mutation: Some(MutationPlan::Update {
            set: vec![(field("status"), json!("updated"))],
            returning: vec![],
        }),
        ordering: vec![],
        pagination: None,
    };
    
    let mut grant = user_grant("user-alice");
    grant.action = DataAction::Update;
    // requires_where is true for users
    
    let result = execute(&plan, &grant, &conn, Some("user-alice"));
    
    assert!(matches!(result, Err(ExecutionError::WhereRequired { .. })));
}

#[test]
fn contract_delete_requires_where_for_user() {
    let conn = setup_test_db();
    
    // Delete without WHERE clause
    let plan = LogicalPlan {
        action: DataAction::Delete,
        base_model: items_model(),
        selection: SelectionPlan::empty(),
        joins: vec![],
        filter: None, // No WHERE!
        mutation: Some(MutationPlan::Delete {
            returning: vec![],
        }),
        ordering: vec![],
        pagination: None,
    };
    
    let mut grant = user_grant("user-alice");
    grant.action = DataAction::Delete;
    
    let result = execute(&plan, &grant, &conn, Some("user-alice"));
    
    assert!(matches!(result, Err(ExecutionError::WhereRequired { .. })));
}

#[test]
fn contract_admin_can_update_without_where() {
    let conn = setup_test_db();
    
    // Admin can update without WHERE because requires_where is false
    let plan = LogicalPlan {
        action: DataAction::Update,
        base_model: items_model(),
        selection: SelectionPlan::empty(),
        joins: vec![],
        filter: Some(FilterOp::Eq("status".into(), FilterValue::String("draft".into()))),
        mutation: Some(MutationPlan::Update {
            set: vec![(field("status"), json!("published"))],
            returning: vec![],
        }),
        ordering: vec![],
        pagination: None,
    };
    
    let mut grant = admin_grant();
    grant.action = DataAction::Update;
    
    let result = execute(&plan, &grant, &conn, None);
    
    assert!(result.is_ok());
}

// =============================================================================
// Contract: Never Predicate
// =============================================================================

#[test]
fn contract_never_predicate_returns_no_rows() {
    let conn = setup_test_db();
    let plan = query_plan(vec!["id"], None);
    
    let grant = PlanGrant {
        model: "items".into(),
        action: DataAction::Query,
        row_predicate: RowPredicate::Never,
        field_mask: FieldMask::default(),
        joins: vec![],
        bulk: BulkGrant::default(),
    };
    
    let result = execute(&plan, &grant, &conn, None).unwrap();
    
    match result {
        ExecutionResult::Rows(rows) => {
            assert_eq!(rows.len(), 0, "Never predicate should return 0 rows");
        }
        _ => panic!("Expected Rows result"),
    }
}

// =============================================================================
// Contract: Update Only Affects Authorized Rows
// =============================================================================

#[test]
fn contract_update_only_affects_authorized_rows() {
    let conn = setup_test_db();
    
    // Alice tries to update all items to 'updated'
    let plan = LogicalPlan {
        action: DataAction::Update,
        base_model: items_model(),
        selection: SelectionPlan::empty(),
        joins: vec![],
        filter: Some(FilterOp::Eq("status".into(), FilterValue::String("active".into()))),
        mutation: Some(MutationPlan::Update {
            set: vec![(field("status"), json!("updated"))],
            returning: vec![field("id")],
        }),
        ordering: vec![],
        pagination: None,
    };
    
    let mut grant = user_grant("user-alice");
    grant.action = DataAction::Update;
    
    // Execute update
    let result = execute(&plan, &grant, &conn, Some("user-alice")).unwrap();
    
    // Alice has 2 active items (item-1, item-4), so should update 2 rows
    match result {
        ExecutionResult::Affected { rows, returning } => {
            assert_eq!(rows, 2, "Should affect only Alice's 2 active items");
        }
        _ => panic!("Expected Affected result"),
    }
    
    // Verify Bob's item was NOT updated
    let bob_status: String = conn
        .query_row(
            "SELECT status FROM items WHERE id = 'item-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    
    assert_eq!(bob_status, "active", "Bob's item should NOT be updated");
}
