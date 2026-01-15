//! LogicalPlan — the output of the Planner.
//!
//! This is **policy-agnostic**. It represents what the request *means*,
//! not whether it's *allowed*.

use serde::{Deserialize, Serialize};

use crate::protocol::data::{DataAction, FilterOp, JoinType};

use super::types::{FieldRef, ModelRef, RelationRef, RowLiteral};

/// The complete logical plan for a data operation.
///
/// Two identical requests **must produce identical LogicalPlans**.
/// This matters for caching and testing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogicalPlan {
    /// The action being performed.
    pub action: DataAction,

    /// The base model being operated on.
    pub base_model: ModelRef,

    /// Field selection for queries.
    pub selection: SelectionPlan,

    /// Resolved joins.
    pub joins: Vec<JoinPlan>,

    /// Resolved filter (normalized).
    pub filter: Option<FilterOp>,

    /// Mutation details (for insert/update/delete).
    pub mutation: Option<MutationPlan>,

    /// Ordering specification.
    pub ordering: Vec<OrderPlan>,

    /// Pagination (limit/offset).
    pub pagination: Option<PaginationPlan>,
}

impl LogicalPlan {
    /// Check if this plan involves any joins.
    pub fn has_joins(&self) -> bool {
        !self.joins.is_empty()
    }

    /// Check if this plan has a filter.
    pub fn has_filter(&self) -> bool {
        self.filter.is_some()
    }

    /// Get all aliases declared in this plan.
    pub fn declared_aliases(&self) -> Vec<&str> {
        self.joins.iter().map(|j| j.alias.as_str()).collect()
    }

    /// Check if a mutation plan exists.
    pub fn is_mutation(&self) -> bool {
        self.mutation.is_some()
    }
}

/// Field selection plan.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SelectionPlan {
    /// Fields to select from the base model.
    pub base_fields: Vec<FieldRef>,
}

impl SelectionPlan {
    /// Create an empty selection.
    pub fn empty() -> Self {
        Self { base_fields: vec![] }
    }

    /// Create a selection with fields.
    pub fn new(fields: Vec<FieldRef>) -> Self {
        Self { base_fields: fields }
    }
}

/// Resolved join plan.
///
/// Guarantees:
/// - Join exists in __relations
/// - FK direction is correct
/// - Alias is unique
/// - Fields belong to the joined model
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinPlan {
    /// Resolved relation reference.
    pub relation: RelationRef,

    /// Alias for the joined table.
    pub alias: String,

    /// Join type (inner/left).
    pub join_type: JoinType,

    /// Fields to select from the joined table.
    pub selected_fields: Vec<FieldRef>,

    /// Optional filter on joined rows.
    pub filter: Option<FilterOp>,
}

impl JoinPlan {
    /// Create a new join plan.
    pub fn new(
        relation: RelationRef,
        alias: impl Into<String>,
        join_type: JoinType,
        selected_fields: Vec<FieldRef>,
    ) -> Self {
        Self {
            relation,
            alias: alias.into(),
            join_type,
            selected_fields,
            filter: None,
        }
    }

    /// Add a filter to the join.
    pub fn with_filter(mut self, filter: FilterOp) -> Self {
        self.filter = Some(filter);
        self
    }
}

/// Mutation plan variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MutationPlan {
    /// Insert rows.
    Insert {
        /// Row data (values still JSON).
        rows: Vec<RowLiteral>,
        /// Fields to return after insert.
        returning: Vec<FieldRef>,
    },

    /// Update matching rows.
    Update {
        /// Fields to set (resolved field + value).
        set: Vec<(FieldRef, serde_json::Value)>,
        /// Fields to return after update.
        returning: Vec<FieldRef>,
    },

    /// Delete matching rows.
    Delete {
        /// Fields to return (usually just IDs).
        returning: Vec<FieldRef>,
    },
}

/// Ordering plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderPlan {
    /// Field to order by.
    pub field: FieldRef,
    /// Whether descending.
    pub descending: bool,
}

impl OrderPlan {
    /// Create an ascending order.
    pub fn asc(field: FieldRef) -> Self {
        Self {
            field,
            descending: false,
        }
    }

    /// Create a descending order.
    pub fn desc(field: FieldRef) -> Self {
        Self {
            field,
            descending: true,
        }
    }
}

/// Pagination plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaginationPlan {
    /// Maximum rows to return.
    pub limit: u32,
    /// Offset for pagination.
    pub offset: u32,
}

impl PaginationPlan {
    /// Create a new pagination plan.
    pub fn new(limit: u32, offset: u32) -> Self {
        Self { limit, offset }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::types::FieldType;

    fn test_model() -> ModelRef {
        ModelRef::new("items", "model-1")
    }

    fn test_field(name: &str) -> FieldRef {
        FieldRef::new(test_model(), name, FieldType::String)
    }

    #[test]
    fn test_logical_plan_has_joins() {
        let plan = LogicalPlan {
            action: DataAction::Query,
            base_model: test_model(),
            selection: SelectionPlan::empty(),
            joins: vec![],
            filter: None,
            mutation: None,
            ordering: vec![],
            pagination: None,
        };
        assert!(!plan.has_joins());
    }

    #[test]
    fn test_selection_plan() {
        let selection = SelectionPlan::new(vec![
            test_field("id"),
            test_field("name"),
        ]);
        assert_eq!(selection.base_fields.len(), 2);
    }

    #[test]
    fn test_order_plan() {
        let asc = OrderPlan::asc(test_field("created_at"));
        assert!(!asc.descending);

        let desc = OrderPlan::desc(test_field("created_at"));
        assert!(desc.descending);
    }

    #[test]
    fn test_pagination_plan() {
        let page = PaginationPlan::new(50, 100);
        assert_eq!(page.limit, 50);
        assert_eq!(page.offset, 100);
    }
}
