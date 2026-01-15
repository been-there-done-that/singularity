//! Stage 2: Schema resolution.
//!
//! Resolves raw strings to typed references against the schema:
//! - Model name → ModelRef
//! - Field name → FieldRef
//! - Relation name → RelationRef
//! - Alias → joined model

use crate::protocol::data::{FilterOp, JoinSpec, OrderDir, OrderSpec};

use super::error::PlannerError;
use super::logical_plan::{JoinPlan, OrderPlan, PaginationPlan, SelectionPlan};
use super::types::{FieldRef, FieldType, ModelRef, RelationRef};
use super::SchemaView;

/// Resolve model name to ModelRef.
pub fn resolve_model(name: &str, schema: &impl SchemaView) -> Result<ModelRef, PlannerError> {
    schema.get_model(name).ok_or_else(|| PlannerError::UnknownModel {
        name: name.to_string(),
    })
}

/// Resolve a list of field names to FieldRefs.
pub fn resolve_field_list(
    model: &ModelRef,
    fields: &[String],
    schema: &impl SchemaView,
) -> Result<Vec<FieldRef>, PlannerError> {
    fields
        .iter()
        .map(|f| resolve_single_field(model, f, schema))
        .collect()
}

/// Resolve a single field name to FieldRef.
fn resolve_single_field(
    model: &ModelRef,
    field_name: &str,
    schema: &impl SchemaView,
) -> Result<FieldRef, PlannerError> {
    // Handle wildcard
    if field_name == "*" {
        // Return a special marker - executor will expand this
        return Ok(FieldRef::new(model.clone(), "*", FieldType::Unknown));
    }

    schema
        .get_field(model, field_name)
        .ok_or_else(|| PlannerError::UnknownField {
            model: model.name.clone(),
            field: field_name.to_string(),
        })
}

/// Resolve selection fields.
pub fn resolve_selection(
    model: &ModelRef,
    fields: &[String],
    schema: &impl SchemaView,
) -> Result<SelectionPlan, PlannerError> {
    let resolved = resolve_field_list(model, fields, schema)?;
    Ok(SelectionPlan::new(resolved))
}

/// Resolve joins.
pub fn resolve_joins(
    joins: &[JoinSpec],
    schema: &impl SchemaView,
) -> Result<Vec<JoinPlan>, PlannerError> {
    joins.iter().map(|j| resolve_single_join(j, schema)).collect()
}

/// Resolve a single join.
fn resolve_single_join(
    join: &JoinSpec,
    schema: &impl SchemaView,
) -> Result<JoinPlan, PlannerError> {
    // Look up the relation
    let relation = schema
        .get_relation(&join.relation)
        .ok_or_else(|| PlannerError::UnknownRelation {
            name: join.relation.clone(),
        })?;

    // Resolve selected fields from the target model
    let selected_fields = resolve_field_list(&relation.to_model, &join.select, schema)?;

    // Create the join plan
    let mut plan = JoinPlan::new(
        relation,
        &join.r#as,
        join.r#type.clone(),
        selected_fields,
    );

    // Add filter if present (will be validated in filter resolution)
    if join.r#where.is_some() {
        plan.filter = join.r#where.clone();
    }

    Ok(plan)
}

/// Resolve filter, checking field references against declared aliases.
pub fn resolve_filter(
    base_model: &ModelRef,
    filter: &Option<FilterOp>,
    joins: &[JoinPlan],
    schema: &impl SchemaView,
) -> Result<Option<FilterOp>, PlannerError> {
    let filter = match filter {
        Some(f) => f,
        None => return Ok(None),
    };

    // Build alias map
    let alias_map: std::collections::HashMap<&str, &ModelRef> = joins
        .iter()
        .map(|j| (j.alias.as_str(), &j.relation.to_model))
        .collect();

    // Validate all field references in the filter
    validate_filter_fields(filter, base_model, &alias_map, schema)?;

    Ok(Some(filter.clone()))
}

/// Recursively validate field references in a filter.
fn validate_filter_fields(
    filter: &FilterOp,
    base_model: &ModelRef,
    alias_map: &std::collections::HashMap<&str, &ModelRef>,
    schema: &impl SchemaView,
) -> Result<(), PlannerError> {
    match filter {
        FilterOp::And(filters) | FilterOp::Or(filters) => {
            for f in filters {
                validate_filter_fields(f, base_model, alias_map, schema)?;
            }
        }
        FilterOp::Not(inner) => {
            validate_filter_fields(inner, base_model, alias_map, schema)?;
        }
        // Comparison operators with field references
        FilterOp::Eq(field, _)
        | FilterOp::Ne(field, _)
        | FilterOp::Gt(field, _)
        | FilterOp::Gte(field, _)
        | FilterOp::Lt(field, _)
        | FilterOp::Lte(field, _) => {
            validate_field_reference(field, base_model, alias_map, schema)?;
        }
        FilterOp::In(field, _) | FilterOp::Like(field, _) | FilterOp::ILike(field, _) => {
            validate_field_reference(field, base_model, alias_map, schema)?;
        }
        FilterOp::IsNull(field) | FilterOp::IsNotNull(field) => {
            validate_field_reference(field, base_model, alias_map, schema)?;
        }
    }
    Ok(())
}

/// Validate a single field reference (possibly with alias).
fn validate_field_reference(
    field: &str,
    base_model: &ModelRef,
    alias_map: &std::collections::HashMap<&str, &ModelRef>,
    schema: &impl SchemaView,
) -> Result<(), PlannerError> {
    let (alias, field_name) = FilterOp::parse_field_ref(field);

    let model = match alias {
        Some(a) => {
            alias_map
                .get(a)
                .copied()
                .ok_or_else(|| PlannerError::UndeclaredAlias {
                    alias: a.to_string(),
                })?
        }
        None => base_model,
    };

    // Check that the field exists on the model
    if schema.get_field(model, field_name).is_none() {
        return Err(PlannerError::UnknownField {
            model: model.name.clone(),
            field: field_name.to_string(),
        });
    }

    Ok(())
}

/// Resolve ordering.
pub fn resolve_ordering(
    base_model: &ModelRef,
    order_by: &[OrderSpec],
    joins: &[JoinPlan],
    schema: &impl SchemaView,
) -> Result<Vec<OrderPlan>, PlannerError> {
    // Build alias map for order fields
    let alias_map: std::collections::HashMap<&str, &ModelRef> = joins
        .iter()
        .map(|j| (j.alias.as_str(), &j.relation.to_model))
        .collect();

    order_by
        .iter()
        .map(|spec| {
            let (alias, field_name) = FilterOp::parse_field_ref(&spec.field);

            let model = match alias {
                Some(a) => {
                    alias_map
                        .get(a)
                        .copied()
                        .ok_or_else(|| PlannerError::UndeclaredAlias {
                            alias: a.to_string(),
                        })?
                }
                None => base_model,
            };

            let field = schema
                .get_field(model, field_name)
                .ok_or_else(|| PlannerError::UnknownField {
                    model: model.name.clone(),
                    field: field_name.to_string(),
                })?;

            Ok(OrderPlan {
                field,
                descending: spec.dir == OrderDir::Desc,
            })
        })
        .collect()
}

/// Resolve pagination.
pub fn resolve_pagination(limit: Option<u32>, offset: Option<u32>) -> Option<PaginationPlan> {
    limit.map(|l| PaginationPlan::new(l, offset.unwrap_or(0)))
}

/// Resolve set fields for update.
pub fn resolve_set_fields(
    model: &ModelRef,
    set: &serde_json::Value,
    schema: &impl SchemaView,
) -> Result<Vec<(FieldRef, serde_json::Value)>, PlannerError> {
    let obj = set.as_object().ok_or_else(|| PlannerError::InvalidFilter {
        reason: "set must be a JSON object".into(),
    })?;

    obj.iter()
        .map(|(field_name, value)| {
            let field = schema
                .get_field(model, field_name)
                .ok_or_else(|| PlannerError::UnknownField {
                    model: model.name.clone(),
                    field: field_name.clone(),
                })?;
            Ok((field, value.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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

            let mut fields = HashMap::new();
            fields.insert(
                "items".to_string(),
                vec![
                    FieldRef::new(items_model.clone(), "id", FieldType::String),
                    FieldRef::new(items_model.clone(), "title", FieldType::String),
                    FieldRef::new(items_model.clone(), "price", FieldType::Int),
                    FieldRef::new(items_model.clone(), "owner_id", FieldType::String),
                ],
            );
            fields.insert(
                "users".to_string(),
                vec![
                    FieldRef::new(users_model.clone(), "id", FieldType::String),
                    FieldRef::new(users_model.clone(), "email", FieldType::String),
                ],
            );

            let mut models = HashMap::new();
            models.insert("items".to_string(), items_model.clone());
            models.insert("users".to_string(), users_model.clone());

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

    #[test]
    fn test_resolve_model_success() {
        let schema = TestSchema::new();
        let model = resolve_model("items", &schema).unwrap();
        assert_eq!(model.name, "items");
    }

    #[test]
    fn test_resolve_model_unknown() {
        let schema = TestSchema::new();
        let err = resolve_model("nonexistent", &schema).unwrap_err();
        assert!(matches!(err, PlannerError::UnknownModel { .. }));
    }

    #[test]
    fn test_resolve_field_list() {
        let schema = TestSchema::new();
        let model = resolve_model("items", &schema).unwrap();
        let fields = resolve_field_list(&model, &["id".into(), "title".into()], &schema).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "id");
        assert_eq!(fields[1].name, "title");
    }

    #[test]
    fn test_resolve_unknown_field() {
        let schema = TestSchema::new();
        let model = resolve_model("items", &schema).unwrap();
        let err = resolve_field_list(&model, &["nonexistent".into()], &schema).unwrap_err();
        assert!(matches!(err, PlannerError::UnknownField { .. }));
    }

    #[test]
    fn test_resolve_join() {
        let schema = TestSchema::new();
        let join_spec = JoinSpec {
            relation: "items.owner".into(),
            r#as: "owner".into(),
            r#type: Default::default(),
            select: vec!["email".into()],
            r#where: None,
        };
        let joins = resolve_joins(&[join_spec], &schema).unwrap();
        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0].alias, "owner");
        assert_eq!(joins[0].selected_fields[0].name, "email");
    }

    #[test]
    fn test_resolve_unknown_relation() {
        let schema = TestSchema::new();
        let join_spec = JoinSpec {
            relation: "nonexistent.relation".into(),
            r#as: "alias".into(),
            r#type: Default::default(),
            select: vec![],
            r#where: None,
        };
        let err = resolve_joins(&[join_spec], &schema).unwrap_err();
        assert!(matches!(err, PlannerError::UnknownRelation { .. }));
    }

    #[test]
    fn test_filter_with_valid_alias() {
        use crate::protocol::data::FilterValue;

        let schema = TestSchema::new();
        let model = resolve_model("items", &schema).unwrap();
        let join_spec = JoinSpec {
            relation: "items.owner".into(),
            r#as: "owner".into(),
            r#type: Default::default(),
            select: vec!["email".into()],
            r#where: None,
        };
        let joins = resolve_joins(&[join_spec], &schema).unwrap();

        let filter = FilterOp::Eq("owner.email".into(), FilterValue::String("test@test.com".into()));
        let result = resolve_filter(&model, &Some(filter), &joins, &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_filter_with_undeclared_alias() {
        use crate::protocol::data::FilterValue;

        let schema = TestSchema::new();
        let model = resolve_model("items", &schema).unwrap();

        let filter = FilterOp::Eq("unknown.field".into(), FilterValue::String("value".into()));
        let err = resolve_filter(&model, &Some(filter), &[], &schema).unwrap_err();
        assert!(matches!(err, PlannerError::UndeclaredAlias { .. }));
    }
}
