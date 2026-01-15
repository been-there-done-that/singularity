//! Query DSL structures.
//!
//! Defines the canonical query input format including joins and ordering.

use serde::{Deserialize, Serialize};

use super::filter::FilterOp;

/// Query input specification.
///
/// # Example JSON
///
/// ```json
/// {
///   "select": ["id", "title", "price"],
///   "where": { "gt": ["price", 10] },
///   "joins": [{
///     "relation": "items.owner",
///     "as": "owner",
///     "type": "left",
///     "select": ["id", "email"]
///   }],
///   "order_by": [{ "field": "created_at", "dir": "desc" }],
///   "limit": 50,
///   "offset": 0
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryInput {
    /// Fields to select (required)
    pub select: Vec<String>,

    /// Filter condition (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#where: Option<FilterOp>,

    /// Join specifications (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joins: Vec<JoinSpec>,

    /// Ordering (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order_by: Vec<OrderSpec>,

    /// Maximum rows to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,

    /// Offset for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// Join specification.
///
/// Joins are FK-only and must reference a declared relation in `__relations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinSpec {
    /// Relation key from __relations (e.g., "items.owner")
    pub relation: String,

    /// Alias for the joined table (used in field references like "owner.email")
    pub r#as: String,

    /// Join type (default: left)
    #[serde(default)]
    pub r#type: JoinType,

    /// Fields to select from joined table
    pub select: Vec<String>,

    /// Additional filter on joined rows
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#where: Option<FilterOp>,
}

/// Join type.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JoinType {
    /// Left outer join (default - includes rows without matches)
    #[default]
    Left,
    /// Inner join (only rows with matches)
    Inner,
}

/// Ordering specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderSpec {
    /// Field to order by (can be aliased like "owner.created_at")
    pub field: String,
    /// Direction (default: asc)
    #[serde(default)]
    pub dir: OrderDir,
}

/// Order direction.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderDir {
    /// Ascending (default)
    #[default]
    Asc,
    /// Descending
    Desc,
}

impl QueryInput {
    /// Create a minimal query input with just field selection.
    pub fn new(select: Vec<String>) -> Self {
        Self {
            select,
            r#where: None,
            joins: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
        }
    }

    /// Add a filter condition.
    pub fn with_filter(mut self, filter: FilterOp) -> Self {
        self.r#where = Some(filter);
        self
    }

    /// Add a join.
    pub fn with_join(mut self, join: JoinSpec) -> Self {
        self.joins.push(join);
        self
    }

    /// Add ordering.
    pub fn with_order(mut self, field: impl Into<String>, dir: OrderDir) -> Self {
        self.order_by.push(OrderSpec {
            field: field.into(),
            dir,
        });
        self
    }

    /// Set limit and offset for pagination.
    pub fn with_pagination(mut self, limit: u32, offset: u32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }

    /// Get all aliases declared in this query (for field resolution).
    pub fn declared_aliases(&self) -> Vec<&str> {
        self.joins.iter().map(|j| j.r#as.as_str()).collect()
    }
}

impl JoinSpec {
    /// Create a new join specification.
    pub fn new(relation: impl Into<String>, alias: impl Into<String>, select: Vec<String>) -> Self {
        Self {
            relation: relation.into(),
            r#as: alias.into(),
            r#type: JoinType::Left,
            select,
            r#where: None,
        }
    }

    /// Set join type to inner.
    pub fn inner(mut self) -> Self {
        self.r#type = JoinType::Inner;
        self
    }

    /// Add join-level filter.
    pub fn with_filter(mut self, filter: FilterOp) -> Self {
        self.r#where = Some(filter);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::data::FilterValue;

    #[test]
    fn test_query_input_minimal() {
        let q = QueryInput::new(vec!["id".into(), "name".into()]);
        let json = serde_json::to_string(&q).unwrap();
        assert!(json.contains("\"select\""));
        assert!(!json.contains("\"where\""));
    }

    #[test]
    fn test_query_input_full() {
        let q = QueryInput::new(vec!["id".into(), "title".into()])
            .with_filter(FilterOp::Gt("price".into(), FilterValue::Int(10)))
            .with_join(JoinSpec::new("items.owner", "owner", vec!["email".into()]))
            .with_order("created_at", OrderDir::Desc)
            .with_pagination(50, 0);

        let json = serde_json::to_string_pretty(&q).unwrap();
        let back: QueryInput = serde_json::from_str(&json).unwrap();
        assert_eq!(q, back);
    }

    #[test]
    fn test_declared_aliases() {
        let q = QueryInput::new(vec!["id".into()])
            .with_join(JoinSpec::new("items.owner", "owner", vec!["email".into()]))
            .with_join(JoinSpec::new("items.category", "cat", vec!["name".into()]));
        
        let aliases = q.declared_aliases();
        assert_eq!(aliases, vec!["owner", "cat"]);
    }

    #[test]
    fn test_join_type_serialization() {
        assert_eq!(serde_json::to_string(&JoinType::Left).unwrap(), "\"left\"");
        assert_eq!(serde_json::to_string(&JoinType::Inner).unwrap(), "\"inner\"");
    }
}
