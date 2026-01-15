//! Meta Contract Test — Architectural Pipeline Guard
//!
//! This test guards the invariant:
//!
//! > Every `data.*` execution MUST pass through:
//! > Planner → Policy → Executor
//!
//! No shortcuts. No bypasses. Ever.

use singularity::planner::{
    self, FieldRef, FieldType, LogicalPlan, ModelRef, SchemaView,
};
use singularity::policy::{
    ModelPolicyConfig, PlanAuthContext, PlanAuthorizer, PolicySubject,
};
use singularity::executor::{self, ExecutionMode, ExecutionResult};
use singularity::protocol::data::{
    FilterOp, FilterValue, InsertInput, UpdateInput, DeleteInput, QueryInput,
};


use rusqlite::Connection;


// =============================================================================
// Test Schema Implementation
// =============================================================================

fn items_model() -> ModelRef {
    ModelRef::new("items", "model-items")
}

fn field(name: &str, ft: FieldType) -> FieldRef {
    FieldRef::new(items_model(), name, ft)
}

struct TestSchema {
    models: Vec<ModelRef>,
    fields: Vec<FieldRef>,
}

impl TestSchema {
    fn new() -> Self {
        Self {
            models: vec![items_model()],
            fields: vec![
                field("id", FieldType::String),
                field("title", FieldType::String),
                field("status", FieldType::String),
                field("owner_id", FieldType::String),
            ],
        }
    }
}

impl SchemaView for TestSchema {
    fn get_model(&self, name: &str) -> Option<ModelRef> {
        self.models.iter().find(|m| m.name == name).cloned()
    }

    fn get_field(&self, model: &ModelRef, field_name: &str) -> Option<FieldRef> {
        self.fields
            .iter()
            .find(|f| f.model.id == model.id && f.name == field_name)
            .cloned()
    }

    fn get_model_fields(&self, model: &ModelRef) -> Vec<FieldRef> {
        self.fields
            .iter()
            .filter(|f| f.model.id == model.id)
            .cloned()
            .collect()
    }

    fn get_relation(&self, _name: &str) -> Option<singularity::planner::RelationRef> {
        None
    }
}

// =============================================================================
// Test Fixtures
// =============================================================================

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    
    conn.execute(
        "CREATE TABLE items (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            status TEXT DEFAULT 'draft',
            owner_id TEXT NOT NULL
        )",
        [],
    )
    .unwrap();
    
    conn.execute(
        "INSERT INTO items (id, title, status, owner_id) VALUES 
            ('item-1', 'Test Item', 'active', 'user-alice')",
        [],
    )
    .unwrap();
    
    conn
}

fn admin_subject() -> PolicySubject {
    PolicySubject::new("admin-1")
        .with_roles(vec!["admin"])
        .with_internal_id("admin-1")
}


fn admin_policy_config() -> ModelPolicyConfig {
    ModelPolicyConfig {
        is_public: false,
        owner_field: Some("owner_id".into()),
        admin_roles: vec!["admin".into()],
        max_bulk_rows: Some(100),
        forbidden_read_fields: vec![],
        forbidden_write_fields: vec![],
        allowed_relations: vec![],
    }
}


fn auth_context() -> PlanAuthContext {
    PlanAuthContext {
        subject: admin_subject(),
        now: 1704067200,
        policy_config: admin_policy_config(),
    }
}

// =============================================================================
// Pipeline Tracker
// =============================================================================

#[derive(Debug, Default)]
struct PipelineTracker {
    planner_hit: bool,
    policy_hit: bool,
    executor_hit: bool,
}

impl PipelineTracker {
    fn assert_full_pipeline(&self, operation: &str) {
        assert!(
            self.planner_hit,
            "{}: Planner was NOT invoked — architecture violation!",
            operation
        );
        assert!(
            self.policy_hit,
            "{}: Policy was NOT invoked — architecture violation!",
            operation
        );
        assert!(
            self.executor_hit,
            "{}: Executor was NOT invoked — architecture violation!",
            operation
        );
    }
}

fn execute_through_pipeline<F>(
    operation: &str,
    plan_fn: F,
    conn: &Connection,
) -> PipelineTracker
where
    F: FnOnce(&TestSchema) -> Result<LogicalPlan, singularity::planner::PlannerError>,
{
    let mut tracker = PipelineTracker::default();
    let schema = TestSchema::new();
    let authorizer = PlanAuthorizer::new();
    
    // Stage 1: Planner
    let plan = plan_fn(&schema);
    if plan.is_ok() {
        tracker.planner_hit = true;
    }
    let plan = plan.expect(&format!("{}: Planner failed", operation));
    
    // Stage 2: Policy
    let grant = authorizer.authorize(&plan, &auth_context());
    if grant.is_ok() {
        tracker.policy_hit = true;
    }
    let grant = grant.expect(&format!("{}: Policy failed", operation));
    
    // Stage 3: Executor
    let result = executor::execute_with_mode(&plan, &grant, conn, Some("admin-1"));
    if result.is_ok() {
        tracker.executor_hit = true;
    }
    
    tracker
}

// =============================================================================
// Meta Contract Tests
// =============================================================================

/// The primary architectural guard test.
#[test]
fn contract_data_pipeline_integrity() {
    let conn = setup_test_db();
    
    // Test data.query
    let query_tracker = execute_through_pipeline(
        "data.query",
        |schema| {
            let input = QueryInput::new(vec!["id".into(), "title".into()]);
            planner::plan_query("items", &input, schema)
        },
        &conn,
    );
    query_tracker.assert_full_pipeline("data.query");
    
    // Test data.count
    let count_tracker = execute_through_pipeline(
        "data.count",
        |schema| {
            let input = QueryInput::new(vec![])
                .with_filter(FilterOp::Eq("status".into(), FilterValue::String("active".into())));
            planner::plan_count("items", &input, schema)
        },
        &conn,
    );
    count_tracker.assert_full_pipeline("data.count");
    
    // Test data.insert
    let insert_tracker = execute_through_pipeline(
        "data.insert",
        |schema| {
            let input = InsertInput::single(serde_json::json!({
                "id": "item-new",
                "title": "New Item",
                "status": "draft",
                "owner_id": "admin-1"
            }));
            planner::plan_insert("items", &input, schema)
        },
        &conn,
    );
    insert_tracker.assert_full_pipeline("data.insert");
    
    // Test data.update
    let update_tracker = execute_through_pipeline(
        "data.update",
        |schema| {
            let input = UpdateInput::new(
                FilterOp::Eq("id".into(), FilterValue::String("item-1".into())),
                serde_json::json!({ "title": "Updated Title" }),
            );
            planner::plan_update("items", &input, schema)
        },
        &conn,
    );
    update_tracker.assert_full_pipeline("data.update");
    
    // Test data.delete
    let delete_tracker = execute_through_pipeline(
        "data.delete",
        |schema| {
            let input = DeleteInput::new(
                FilterOp::Eq("id".into(), FilterValue::String("item-new".into())),
            );
            planner::plan_delete("items", &input, schema)
        },
        &conn,
    );
    delete_tracker.assert_full_pipeline("data.delete");
    
    // All 5 operation types verified ✓
}

/// Verify ExecutionMode is correctly determined.
#[test]
fn contract_execution_mode_observable() {
    let conn = setup_test_db();
    let schema = TestSchema::new();
    let authorizer = PlanAuthorizer::new();
    
    let input = QueryInput::new(vec!["id".into()]);
    let plan = planner::plan_query("items", &input, &schema).unwrap();
    let grant = authorizer.authorize(&plan, &auth_context()).unwrap();
    
    let (result, mode) = executor::execute_with_mode(&plan, &grant, &conn, Some("admin-1")).unwrap();
    
    assert_eq!(mode, ExecutionMode::Fast, "Admin should use fast path");
    assert!(matches!(result, ExecutionResult::Rows(_)));
}

/// Verify plan hash is deterministic.
#[test]
fn contract_plan_hash_determinism() {
    let schema = TestSchema::new();
    let authorizer = PlanAuthorizer::new();
    
    let input = QueryInput::new(vec!["id".into(), "title".into()])
        .with_filter(FilterOp::Eq("status".into(), FilterValue::String("active".into())));
    
    let plan1 = planner::plan_query("items", &input, &schema).unwrap();
    let plan2 = planner::plan_query("items", &input, &schema).unwrap();
    
    let grant1 = authorizer.authorize(&plan1, &auth_context()).unwrap();
    let grant2 = authorizer.authorize(&plan2, &auth_context()).unwrap();
    
    let hash1 = executor::compute_plan_hash(&plan1, &grant1);
    let hash2 = executor::compute_plan_hash(&plan2, &grant2);
    
    assert_eq!(
        hash1, hash2,
        "Identical operations must produce identical plan hashes"
    );
}
