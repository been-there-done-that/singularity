//! Planner module — transforms DSL into LogicalPlan.
//!
//! # Responsibility
//!
//! The Planner is a **pure, deterministic transformer**:
//! - Input: `OpRequest` + DSL + schema metadata
//! - Output: `LogicalPlan`
//!
//! The Planner does **NOT**:
//! - Talk to the database
//! - Evaluate Cedar
//! - Execute SQL
//! - Mutate state
//! - Check permissions
//!
//! # Question It Answers
//!
//! > "Is this request structurally valid, and what does it *mean*?"
//!
//! Authorization comes *after* planning.

mod error;
mod logical_plan;
mod resolve;
mod types;
mod validate;

pub use error::PlannerError;
pub use logical_plan::{
    JoinPlan, LogicalPlan, MutationPlan, OrderPlan, PaginationPlan, SelectionPlan,
};
pub use types::{FieldRef, FieldType, ModelRef, RelationRef, RowLiteral};

use crate::protocol::data::{
    DataAction, DeleteInput, FilterOp, InsertInput, QueryInput, UpdateInput,
};

/// Schema snapshot for planning (read-only view).
///
/// The planner never queries the database directly — it receives
/// a snapshot from the caller.
pub trait SchemaView {
    /// Get a model by name.
    fn get_model(&self, name: &str) -> Option<ModelRef>;
    
    /// Get a field by model and field name.
    fn get_field(&self, model: &ModelRef, field_name: &str) -> Option<FieldRef>;
    
    /// Get all fields for a model.
    fn get_model_fields(&self, model: &ModelRef) -> Vec<FieldRef>;
    
    /// Get a relation by name (e.g., "items.owner").
    fn get_relation(&self, name: &str) -> Option<RelationRef>;
}

/// Plan a query operation.
pub fn plan_query(
    model_name: &str,
    input: &QueryInput,
    schema: &impl SchemaView,
) -> Result<LogicalPlan, PlannerError> {
    // Stage 1: Structural validation
    validate::validate_query_structure(input)?;

    // Stage 2: Schema resolution
    let base_model = resolve::resolve_model(model_name, schema)?;
    let selection = resolve::resolve_selection(&base_model, &input.select, schema)?;
    let joins = resolve::resolve_joins(&input.joins, schema)?;
    let filter = resolve::resolve_filter(&base_model, &input.r#where, &joins, schema)?;
    let ordering = resolve::resolve_ordering(&base_model, &input.order_by, &joins, schema)?;
    let pagination = resolve::resolve_pagination(input.limit, input.offset);

    // Stage 3: Build LogicalPlan
    Ok(LogicalPlan {
        action: DataAction::Query,
        base_model,
        selection,
        joins,
        filter,
        mutation: None,
        ordering,
        pagination,
    })
}

/// Plan a count operation.
pub fn plan_count(
    model_name: &str,
    input: &QueryInput,
    schema: &impl SchemaView,
) -> Result<LogicalPlan, PlannerError> {
    // Stage 1: Structural validation (same as query, minus select validation)
    validate::validate_filter_structure(input.r#where.as_ref())?;

    // Stage 2: Schema resolution
    let base_model = resolve::resolve_model(model_name, schema)?;
    let joins = resolve::resolve_joins(&input.joins, schema)?;
    let filter = resolve::resolve_filter(&base_model, &input.r#where, &joins, schema)?;

    // Stage 3: Build LogicalPlan (minimal - just for counting)
    Ok(LogicalPlan {
        action: DataAction::Count,
        base_model,
        selection: SelectionPlan { base_fields: vec![] },
        joins,
        filter,
        mutation: None,
        ordering: vec![],
        pagination: None,
    })
}

/// Plan an insert operation.
pub fn plan_insert(
    model_name: &str,
    input: &InsertInput,
    schema: &impl SchemaView,
) -> Result<LogicalPlan, PlannerError> {
    // Stage 1: Structural validation
    validate::validate_insert_structure(input)?;

    // Stage 2: Schema resolution
    let base_model = resolve::resolve_model(model_name, schema)?;
    let returning = resolve::resolve_field_list(&base_model, &input.returning, schema)?;
    
    // Convert rows to RowLiterals
    let rows: Vec<RowLiteral> = input
        .rows
        .iter()
        .map(|row| RowLiteral { data: row.clone() })
        .collect();

    // Stage 3: Build LogicalPlan
    Ok(LogicalPlan {
        action: DataAction::Insert,
        base_model,
        selection: SelectionPlan { base_fields: vec![] },
        joins: vec![],
        filter: None,
        mutation: Some(MutationPlan::Insert { rows, returning }),
        ordering: vec![],
        pagination: None,
    })
}

/// Plan an update operation.
pub fn plan_update(
    model_name: &str,
    input: &UpdateInput,
    schema: &impl SchemaView,
) -> Result<LogicalPlan, PlannerError> {
    // Stage 1: Structural validation
    validate::validate_update_structure(input)?;

    // Stage 2: Schema resolution
    let base_model = resolve::resolve_model(model_name, schema)?;
    let filter = resolve::resolve_filter(&base_model, &Some(input.r#where.clone()), &[], schema)?;
    let returning = resolve::resolve_field_list(&base_model, &input.returning, schema)?;
    
    // Resolve set fields
    let set = resolve::resolve_set_fields(&base_model, &input.set, schema)?;

    // Stage 3: Build LogicalPlan
    Ok(LogicalPlan {
        action: DataAction::Update,
        base_model,
        selection: SelectionPlan { base_fields: vec![] },
        joins: vec![],
        filter,
        mutation: Some(MutationPlan::Update {
            set,
            returning,
        }),
        ordering: vec![],
        pagination: None,
    })
}

/// Plan a delete operation.
pub fn plan_delete(
    model_name: &str,
    input: &DeleteInput,
    schema: &impl SchemaView,
) -> Result<LogicalPlan, PlannerError> {
    // Stage 1: Structural validation
    validate::validate_delete_structure(input)?;

    // Stage 2: Schema resolution
    let base_model = resolve::resolve_model(model_name, schema)?;
    let filter = resolve::resolve_filter(&base_model, &Some(input.r#where.clone()), &[], schema)?;
    let returning = resolve::resolve_field_list(&base_model, &input.returning, schema)?;

    // Stage 3: Build LogicalPlan
    Ok(LogicalPlan {
        action: DataAction::Delete,
        base_model,
        selection: SelectionPlan { base_fields: vec![] },
        joins: vec![],
        filter,
        mutation: Some(MutationPlan::Delete { returning }),
        ordering: vec![],
        pagination: None,
    })
}
