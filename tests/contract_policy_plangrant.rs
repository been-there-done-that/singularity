//! Contract tests for Plan Authorization (Phase C).
//!
//! These tests verify the policy → PlanGrant contract:
//!
//! **Action-level:**
//! - query allowed → PlanGrant
//! - update denied for unauthenticated → 403
//!
//! **Row predicate:**
//! - admin → Always
//! - user → Sql(eq(owner_id, subject))
//!
//! **Field masking:**
//! - forbidden select field → deny
//! - forbidden update field → deny
//!
//! **Joins:**
//! - join allowed → JoinGrant present
//! - join denied → deny
//! - join allowed but field forbidden → deny
//!
//! **Bulk:**
//! - bulk update with WHERE → ok
//! - user has max_rows limit
//! - admin has no limit

use singularity::planner::{
    FieldRef, FieldType, JoinPlan, LogicalPlan, ModelRef, MutationPlan, RelationRef,
    SelectionPlan,
};
use singularity::policy::{
    AuthorizationError, ModelPolicyConfig, PlanAuthContext, PlanAuthorizer, PolicySubject,
};
use singularity::protocol::data::{
    DataAction, FilterOp, FilterValue, JoinType, RowPredicate,
};
use serde_json::json;

// =============================================================================
// Test Fixtures
// =============================================================================

fn items_model() -> ModelRef {
    ModelRef::new("items", "model-items")
}

fn users_model() -> ModelRef {
    ModelRef::new("users", "model-users")
}

fn field(model: &ModelRef, name: &str) -> FieldRef {
    FieldRef::new(model.clone(), name, FieldType::String)
}

fn query_plan(fields: Vec<&str>) -> LogicalPlan {
    LogicalPlan {
        action: DataAction::Query,
        base_model: items_model(),
        selection: SelectionPlan::new(
            fields.into_iter().map(|f| field(&items_model(), f)).collect(),
        ),
        joins: vec![],
        filter: None,
        mutation: None,
        ordering: vec![],
        pagination: None,
    }
}

fn update_plan(set_fields: Vec<&str>) -> LogicalPlan {
    LogicalPlan {
        action: DataAction::Update,
        base_model: items_model(),
        selection: SelectionPlan::empty(),
        joins: vec![],
        filter: Some(FilterOp::Eq("id".into(), FilterValue::String("123".into()))),
        mutation: Some(MutationPlan::Update {
            set: set_fields
                .into_iter()
                .map(|f| (field(&items_model(), f), json!("value")))
                .collect(),
            returning: vec![field(&items_model(), "id")],
        }),
        ordering: vec![],
        pagination: None,
    }
}

fn delete_plan() -> LogicalPlan {
    LogicalPlan {
        action: DataAction::Delete,
        base_model: items_model(),
        selection: SelectionPlan::empty(),
        joins: vec![],
        filter: Some(FilterOp::Eq("id".into(), FilterValue::String("123".into()))),
        mutation: Some(MutationPlan::Delete {
            returning: vec![field(&items_model(), "id")],
        }),
        ordering: vec![],
        pagination: None,
    }
}

fn admin_ctx() -> PlanAuthContext {
    PlanAuthContext {
        subject: PolicySubject::new("admin-user")
            .with_roles(["admin"])
            .with_internal_id("internal-admin"),
        now: 1704067200,
        policy_config: ModelPolicyConfig {
            admin_roles: vec!["admin".into()],
            owner_field: Some("owner_id".into()),
            allowed_relations: vec!["items.owner".into()],
            ..Default::default()
        },
    }
}

fn user_ctx() -> PlanAuthContext {
    PlanAuthContext {
        subject: PolicySubject::new("regular-user")
            .with_roles(["user"])
            .with_internal_id("internal-user-123"),
        now: 1704067200,
        policy_config: ModelPolicyConfig {
            admin_roles: vec!["admin".into()],
            owner_field: Some("owner_id".into()),
            max_bulk_rows: Some(100),
            allowed_relations: vec!["items.owner".into()],
            forbidden_read_fields: vec!["password_hash".into()],
            forbidden_write_fields: vec!["id".into(), "created_at".into()],
            ..Default::default()
        },
    }
}

fn public_ctx() -> PlanAuthContext {
    PlanAuthContext {
        subject: PolicySubject::new("anonymous"),
        now: 1704067200,
        policy_config: ModelPolicyConfig {
            is_public: true,
            ..Default::default()
        },
    }
}

fn unauthenticated_ctx() -> PlanAuthContext {
    PlanAuthContext {
        subject: PolicySubject::new(""),
        now: 1704067200,
        policy_config: ModelPolicyConfig::default(),
    }
}

// =============================================================================
// Contract: Action Authorization
// =============================================================================

#[test]
fn contract_query_allowed_produces_grant() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_plan(vec!["id", "title"]);
    
    let grant = authorizer.authorize(&plan, &user_ctx()).unwrap();
    assert_eq!(grant.action, DataAction::Query);
    assert_eq!(grant.model, "items");
}

#[test]
fn contract_update_allowed_for_authenticated() {
    let authorizer = PlanAuthorizer::new();
    let plan = update_plan(vec!["title"]);
    
    let grant = authorizer.authorize(&plan, &user_ctx()).unwrap();
    assert_eq!(grant.action, DataAction::Update);
}

#[test]
fn contract_unauthenticated_denied() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_plan(vec!["id"]);
    
    let err = authorizer.authorize(&plan, &unauthenticated_ctx()).unwrap_err();
    assert!(matches!(err, AuthorizationError::ActionDenied { .. }));
}

// =============================================================================
// Contract: Row Predicate
// =============================================================================

#[test]
fn contract_admin_gets_always_predicate() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_plan(vec!["id", "title"]);
    
    let grant = authorizer.authorize(&plan, &admin_ctx()).unwrap();
    assert_eq!(grant.row_predicate, RowPredicate::Always);
}

#[test]
fn contract_user_gets_ownership_sql_predicate() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_plan(vec!["id", "title"]);
    
    let grant = authorizer.authorize(&plan, &user_ctx()).unwrap();
    
    match grant.row_predicate {
        RowPredicate::Sql { filter } => {
            match filter {
                FilterOp::Eq(field, FilterValue::Subject) => {
                    assert_eq!(field, "owner_id");
                }
                other => panic!("Expected Eq with Subject, got {:?}", other),
            }
        }
        other => panic!("Expected Sql predicate, got {:?}", other),
    }
}

#[test]
fn contract_public_model_gets_always_predicate() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_plan(vec!["id"]);
    
    let grant = authorizer.authorize(&plan, &public_ctx()).unwrap();
    assert_eq!(grant.row_predicate, RowPredicate::Always);
}

// =============================================================================
// Contract: Field Masking
// =============================================================================

#[test]
fn contract_forbidden_select_field_denied() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_plan(vec!["id", "password_hash"]);
    
    let err = authorizer.authorize(&plan, &user_ctx()).unwrap_err();
    match err {
        AuthorizationError::FieldDenied { field, .. } => {
            assert_eq!(field, "password_hash");
        }
        other => panic!("Expected FieldDenied, got {:?}", other),
    }
}

#[test]
fn contract_forbidden_write_field_denied() {
    let authorizer = PlanAuthorizer::new();
    let plan = update_plan(vec!["id", "title"]); // id is forbidden
    
    let err = authorizer.authorize(&plan, &user_ctx()).unwrap_err();
    match err {
        AuthorizationError::FieldDenied { field, .. } => {
            assert_eq!(field, "id");
        }
        other => panic!("Expected FieldDenied, got {:?}", other),
    }
}

#[test]
fn contract_allowed_fields_in_grant() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_plan(vec!["id", "title", "price"]);
    
    let grant = authorizer.authorize(&plan, &user_ctx()).unwrap();
    assert!(grant.field_mask.select_allow.contains(&"id".to_string()));
    assert!(grant.field_mask.select_allow.contains(&"title".to_string()));
    assert!(grant.field_mask.select_allow.contains(&"price".to_string()));
}

// =============================================================================
// Contract: Join Authorization
// =============================================================================

fn query_with_join() -> LogicalPlan {
    let relation = RelationRef::new(
        "items.owner",
        items_model(),
        field(&items_model(), "owner_id"),
        users_model(),
        field(&users_model(), "id"),
    );
    
    let mut plan = query_plan(vec!["id", "title"]);
    plan.joins = vec![JoinPlan::new(
        relation,
        "owner",
        JoinType::Left,
        vec![field(&users_model(), "email")],
    )];
    plan
}

#[test]
fn contract_allowed_join_produces_grant() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_with_join();
    
    let grant = authorizer.authorize(&plan, &user_ctx()).unwrap();
    assert_eq!(grant.joins.len(), 1);
    assert_eq!(grant.joins[0].relation, "items.owner");
    assert!(grant.joins[0].select_allow.contains(&"email".to_string()));
}

#[test]
fn contract_forbidden_join_denied() {
    let authorizer = PlanAuthorizer::new();
    
    // Create a join on a relation not in allowed list
    let relation = RelationRef::new(
        "items.secret_relation",
        items_model(),
        field(&items_model(), "secret_id"),
        users_model(),
        field(&users_model(), "id"),
    );
    
    let mut plan = query_plan(vec!["id"]);
    plan.joins = vec![JoinPlan::new(
        relation,
        "secret",
        JoinType::Left,
        vec![field(&users_model(), "email")],
    )];
    
    let err = authorizer.authorize(&plan, &user_ctx()).unwrap_err();
    match err {
        AuthorizationError::JoinDenied { relation, .. } => {
            assert_eq!(relation, "items.secret_relation");
        }
        other => panic!("Expected JoinDenied, got {:?}", other),
    }
}

#[test]
fn contract_join_has_row_predicate() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_with_join();
    
    let grant = authorizer.authorize(&plan, &user_ctx()).unwrap();
    // User should get ownership predicate on joined table too
    assert!(matches!(
        grant.joins[0].row_predicate,
        RowPredicate::Sql { .. }
    ));
}

// =============================================================================
// Contract: Bulk Constraints
// =============================================================================

#[test]
fn contract_user_bulk_has_max_rows() {
    let authorizer = PlanAuthorizer::new();
    let plan = update_plan(vec!["title"]);
    
    let grant = authorizer.authorize(&plan, &user_ctx()).unwrap();
    assert_eq!(grant.bulk.max_rows, Some(100));
    assert!(grant.bulk.requires_where);
}

#[test]
fn contract_admin_bulk_no_limit() {
    let authorizer = PlanAuthorizer::new();
    let plan = update_plan(vec!["title"]);
    
    let grant = authorizer.authorize(&plan, &admin_ctx()).unwrap();
    assert_eq!(grant.bulk.max_rows, None);
    assert!(!grant.bulk.requires_where);
}

#[test]
fn contract_delete_has_bulk_constraints() {
    let authorizer = PlanAuthorizer::new();
    let plan = delete_plan();
    
    let grant = authorizer.authorize(&plan, &user_ctx()).unwrap();
    assert_eq!(grant.action, DataAction::Delete);
    assert!(grant.bulk.requires_where);
    assert_eq!(grant.bulk.max_rows, Some(100));
}

#[test]
fn contract_returning_fields_in_grant() {
    let authorizer = PlanAuthorizer::new();
    let plan = update_plan(vec!["title"]);
    
    let grant = authorizer.authorize(&plan, &user_ctx()).unwrap();
    assert!(grant.bulk.returning_allow.contains(&"id".to_string()));
}

// =============================================================================
// Contract: Determinism
// =============================================================================

#[test]
fn contract_same_input_produces_identical_grants() {
    let authorizer = PlanAuthorizer::new();
    let plan = query_plan(vec!["id", "title", "price"]);
    let ctx = user_ctx();
    
    let grant1 = authorizer.authorize(&plan, &ctx).unwrap();
    let grant2 = authorizer.authorize(&plan, &ctx).unwrap();
    
    assert_eq!(grant1, grant2, "Same input must produce identical PlanGrants");
}
