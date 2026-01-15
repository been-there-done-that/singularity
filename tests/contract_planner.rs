//! Contract tests for the Planner module.
//!
//! These tests verify the Planner's guarantees:
//! - Valid DSL → LogicalPlan
//! - Invalid field → PlannerError::UnknownField
//! - Unknown relation → PlannerError::UnknownRelation
//! - Bulk without WHERE → PlannerError::MissingWhereForBulk (enforced at type level)
//! - Undeclared alias → PlannerError::UndeclaredAlias
//! - Deterministic: same input twice → identical plans

use singularity::planner::{
    plan_query, plan_insert, plan_update, plan_delete, plan_count,
    PlannerError, SchemaView, ModelRef, FieldRef, FieldType, RelationRef,
};
use singularity::protocol::data::{
    QueryInput, InsertInput, UpdateInput, DeleteInput, FilterOp, FilterValue,
    JoinSpec, OrderSpec, OrderDir, DataAction,
};
use serde_json::json;
use std::collections::HashMap;

/// Test schema implementation
struct TestSchema {
    models: HashMap<String, ModelRef>,
    fields: HashMap<String, Vec<FieldRef>>,
    relations: HashMap<String, RelationRef>,
}

impl TestSchema {
    fn new() -> Self {
        let items_model = ModelRef::new("items", "model-items");
        let users_model = ModelRef::new("users", "model-users");
        let categories_model = ModelRef::new("categories", "model-categories");

        let mut fields = HashMap::new();
        fields.insert(
            "items".to_string(),
            vec![
                FieldRef::new(items_model.clone(), "id", FieldType::String),
                FieldRef::new(items_model.clone(), "title", FieldType::String),
                FieldRef::new(items_model.clone(), "price", FieldType::Int),
                FieldRef::new(items_model.clone(), "status", FieldType::String),
                FieldRef::new(items_model.clone(), "owner_id", FieldType::String),
                FieldRef::new(items_model.clone(), "category_id", FieldType::String),
                FieldRef::new(items_model.clone(), "created_at", FieldType::Timestamp),
            ],
        );
        fields.insert(
            "users".to_string(),
            vec![
                FieldRef::new(users_model.clone(), "id", FieldType::String),
                FieldRef::new(users_model.clone(), "email", FieldType::String),
                FieldRef::new(users_model.clone(), "name", FieldType::String),
            ],
        );
        fields.insert(
            "categories".to_string(),
            vec![
                FieldRef::new(categories_model.clone(), "id", FieldType::String),
                FieldRef::new(categories_model.clone(), "name", FieldType::String),
            ],
        );

        let mut models = HashMap::new();
        models.insert("items".to_string(), items_model.clone());
        models.insert("users".to_string(), users_model.clone());
        models.insert("categories".to_string(), categories_model.clone());

        let mut relations = HashMap::new();
        relations.insert(
            "items.owner".to_string(),
            RelationRef::new(
                "items.owner",
                items_model.clone(),
                FieldRef::new(items_model.clone(), "owner_id", FieldType::String),
                users_model.clone(),
                FieldRef::new(users_model.clone(), "id", FieldType::String),
            ),
        );
        relations.insert(
            "items.category".to_string(),
            RelationRef::new(
                "items.category",
                items_model.clone(),
                FieldRef::new(items_model.clone(), "category_id", FieldType::String),
                categories_model.clone(),
                FieldRef::new(categories_model.clone(), "id", FieldType::String),
            ),
        );

        Self {
            models,
            fields,
            relations,
        }
    }
}

impl SchemaView for TestSchema {
    fn get_model(&self, name: &str) -> Option<ModelRef> {
        self.models.get(name).cloned()
    }

    fn get_field(&self, model: &ModelRef, field_name: &str) -> Option<FieldRef> {
        self.fields
            .get(&model.name)?
            .iter()
            .find(|f| f.name == field_name)
            .cloned()
    }

    fn get_model_fields(&self, model: &ModelRef) -> Vec<FieldRef> {
        self.fields.get(&model.name).cloned().unwrap_or_default()
    }

    fn get_relation(&self, name: &str) -> Option<RelationRef> {
        self.relations.get(name).cloned()
    }
}

// =============================================================================
// Contract: Valid DSL → LogicalPlan
// =============================================================================

#[test]
fn test_valid_query_produces_plan() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into(), "title".into(), "price".into()],
        r#where: Some(FilterOp::Gt("price".into(), FilterValue::Int(10))),
        joins: vec![],
        order_by: vec![OrderSpec {
            field: "created_at".into(),
            dir: OrderDir::Desc,
        }],
        limit: Some(50),
        offset: Some(0),
    };

    let plan = plan_query("items", &input, &schema).unwrap();
    
    assert_eq!(plan.action, DataAction::Query);
    assert_eq!(plan.base_model.name, "items");
    assert_eq!(plan.selection.base_fields.len(), 3);
    assert!(plan.has_filter());
    assert_eq!(plan.ordering.len(), 1);
    assert!(plan.ordering[0].descending);
    assert_eq!(plan.pagination.as_ref().unwrap().limit, 50);
}

#[test]
fn test_query_with_join_produces_plan() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into(), "title".into()],
        r#where: None,
        joins: vec![JoinSpec {
            relation: "items.owner".into(),
            r#as: "owner".into(),
            r#type: Default::default(),
            select: vec!["email".into()],
            r#where: None,
        }],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let plan = plan_query("items", &input, &schema).unwrap();
    
    assert!(plan.has_joins());
    assert_eq!(plan.joins.len(), 1);
    assert_eq!(plan.joins[0].alias, "owner");
    assert_eq!(plan.joins[0].selected_fields[0].name, "email");
}

#[test]
fn test_valid_insert_produces_plan() {
    let schema = TestSchema::new();
    let input = InsertInput {
        rows: vec![
            json!({"title": "Item A", "price": 100}),
            json!({"title": "Item B", "price": 200}),
        ],
        returning: vec!["id".into()],
    };

    let plan = plan_insert("items", &input, &schema).unwrap();
    
    assert_eq!(plan.action, DataAction::Insert);
    assert!(plan.is_mutation());
}

#[test]
fn test_valid_update_produces_plan() {
    let schema = TestSchema::new();
    let input = UpdateInput {
        r#where: FilterOp::Eq("status".into(), FilterValue::String("draft".into())),
        set: json!({"status": "published"}),
        returning: vec!["id".into(), "status".into()],
    };

    let plan = plan_update("items", &input, &schema).unwrap();
    
    assert_eq!(plan.action, DataAction::Update);
    assert!(plan.has_filter());
    assert!(plan.is_mutation());
}

#[test]
fn test_valid_delete_produces_plan() {
    let schema = TestSchema::new();
    let input = DeleteInput {
        r#where: FilterOp::Lt("created_at".into(), FilterValue::DateTime(1704067200)),
        returning: vec!["id".into()],
    };

    let plan = plan_delete("items", &input, &schema).unwrap();
    
    assert_eq!(plan.action, DataAction::Delete);
    assert!(plan.has_filter());
    assert!(plan.is_mutation());
}

#[test]
fn test_valid_count_produces_plan() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into()], // Will be ignored for count
        r#where: Some(FilterOp::Eq("status".into(), FilterValue::String("active".into()))),
        joins: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let plan = plan_count("items", &input, &schema).unwrap();
    
    assert_eq!(plan.action, DataAction::Count);
    assert!(plan.has_filter());
}

// =============================================================================
// Contract: Invalid field → PlannerError::UnknownField
// =============================================================================

#[test]
fn test_unknown_field_in_select() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into(), "nonexistent_field".into()],
        r#where: None,
        joins: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let err = plan_query("items", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::UnknownField { field, .. } if field == "nonexistent_field"));
}

#[test]
fn test_unknown_field_in_filter() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into()],
        r#where: Some(FilterOp::Eq("fake_field".into(), FilterValue::Int(1))),
        joins: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let err = plan_query("items", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::UnknownField { field, .. } if field == "fake_field"));
}

#[test]
fn test_unknown_field_in_order() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into()],
        r#where: None,
        joins: vec![],
        order_by: vec![OrderSpec {
            field: "missing_field".into(),
            dir: OrderDir::Asc,
        }],
        limit: None,
        offset: None,
    };

    let err = plan_query("items", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::UnknownField { field, .. } if field == "missing_field"));
}

// =============================================================================
// Contract: Unknown relation → PlannerError::UnknownRelation
// =============================================================================

#[test]
fn test_unknown_relation_in_join() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into()],
        r#where: None,
        joins: vec![JoinSpec {
            relation: "items.nonexistent".into(),
            r#as: "alias".into(),
            r#type: Default::default(),
            select: vec!["id".into()],
            r#where: None,
        }],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let err = plan_query("items", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::UnknownRelation { name } if name == "items.nonexistent"));
}

// =============================================================================
// Contract: Undeclared alias → PlannerError::UndeclaredAlias
// =============================================================================

#[test]
fn test_undeclared_alias_in_filter() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into()],
        r#where: Some(FilterOp::Eq("unknown_alias.field".into(), FilterValue::String("x".into()))),
        joins: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let err = plan_query("items", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::UndeclaredAlias { alias } if alias == "unknown_alias"));
}

#[test]
fn test_undeclared_alias_in_order() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into()],
        r#where: None,
        joins: vec![],
        order_by: vec![OrderSpec {
            field: "fake.created_at".into(),
            dir: OrderDir::Desc,
        }],
        limit: None,
        offset: None,
    };

    let err = plan_query("items", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::UndeclaredAlias { alias } if alias == "fake"));
}

#[test]
fn test_valid_alias_in_filter_with_join() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into()],
        r#where: Some(FilterOp::Eq("owner.email".into(), FilterValue::String("test@test.com".into()))),
        joins: vec![JoinSpec {
            relation: "items.owner".into(),
            r#as: "owner".into(),
            r#type: Default::default(),
            select: vec!["email".into()],
            r#where: None,
        }],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    // Should NOT error - owner alias is declared
    let plan = plan_query("items", &input, &schema).unwrap();
    assert!(plan.has_filter());
}

// =============================================================================
// Contract: Deterministic plan equality
// =============================================================================

#[test]
fn test_same_input_produces_identical_plans() {
    let schema = TestSchema::new();
    
    let make_input = || QueryInput {
        select: vec!["id".into(), "title".into()],
        r#where: Some(FilterOp::And(vec![
            FilterOp::Eq("status".into(), FilterValue::String("active".into())),
            FilterOp::Gt("price".into(), FilterValue::Int(100)),
        ])),
        joins: vec![JoinSpec {
            relation: "items.owner".into(),
            r#as: "owner".into(),
            r#type: Default::default(),
            select: vec!["email".into()],
            r#where: None,
        }],
        order_by: vec![OrderSpec {
            field: "created_at".into(),
            dir: OrderDir::Desc,
        }],
        limit: Some(25),
        offset: Some(50),
    };

    let plan1 = plan_query("items", &make_input(), &schema).unwrap();
    let plan2 = plan_query("items", &make_input(), &schema).unwrap();

    assert_eq!(plan1, plan2, "Same input must produce identical LogicalPlans");
}

// =============================================================================
// Contract: Unknown model → PlannerError::UnknownModel
// =============================================================================

#[test]
fn test_unknown_model() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into()],
        r#where: None,
        joins: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let err = plan_query("nonexistent_model", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::UnknownModel { name } if name == "nonexistent_model"));
}

// =============================================================================
// Contract: Empty select → PlannerError::EmptySelect
// =============================================================================

#[test]
fn test_empty_select() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec![],
        r#where: None,
        joins: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let err = plan_query("items", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::EmptySelect));
}

// =============================================================================
// Contract: Empty insert → PlannerError::EmptyInsert
// =============================================================================

#[test]
fn test_empty_insert() {
    let schema = TestSchema::new();
    let input = InsertInput {
        rows: vec![],
        returning: vec![],
    };

    let err = plan_insert("items", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::EmptyInsert));
}

// =============================================================================
// Contract: Duplicate alias → PlannerError::DuplicateAlias
// =============================================================================

#[test]
fn test_duplicate_alias_in_joins() {
    let schema = TestSchema::new();
    let input = QueryInput {
        select: vec!["id".into()],
        r#where: None,
        joins: vec![
            JoinSpec {
                relation: "items.owner".into(),
                r#as: "related".into(),
                r#type: Default::default(),
                select: vec!["email".into()],
                r#where: None,
            },
            JoinSpec {
                relation: "items.category".into(),
                r#as: "related".into(), // Duplicate!
                r#type: Default::default(),
                select: vec!["name".into()],
                r#where: None,
            },
        ],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let err = plan_query("items", &input, &schema).unwrap_err();
    assert!(matches!(err, PlannerError::DuplicateAlias { alias } if alias == "related"));
}
