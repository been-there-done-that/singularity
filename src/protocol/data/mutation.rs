//! Mutation DSL structures.
//!
//! Defines insert, update, and delete input formats.

use serde::{Deserialize, Serialize};

use super::filter::FilterOp;

/// Insert input for single or bulk inserts.
///
/// # Example JSON
///
/// ```json
/// {
///   "rows": [
///     { "title": "A", "cat": "A" },
///     { "title": "B", "cat": "B" }
///   ],
///   "returning": ["id"]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InsertInput {
    /// Rows to insert
    pub rows: Vec<serde_json::Value>,

    /// Fields to return after insert
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub returning: Vec<String>,
}

/// Update input with filter support.
///
/// # Example JSON
///
/// ```json
/// {
///   "where": { "eq": ["cat", "A"] },
///   "set": { "status": "archived" },
///   "returning": ["id", "status"]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateInput {
    /// Filter for rows to update
    ///
    /// REQUIRED for bulk updates - prevents accidental table-wide mutation.
    pub r#where: FilterOp,

    /// Fields to set
    pub set: serde_json::Value,

    /// Fields to return after update
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub returning: Vec<String>,
}

/// Delete input with filter support.
///
/// # Example JSON
///
/// ```json
/// {
///   "where": { "lt": ["created_at", 1704067200] },
///   "returning": ["id"]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteInput {
    /// Filter for rows to delete
    ///
    /// REQUIRED - prevents accidental table wipes.
    pub r#where: FilterOp,

    /// Fields to return (typically just IDs of deleted rows)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub returning: Vec<String>,
}

impl InsertInput {
    /// Create an insert input with a single row.
    pub fn single(row: serde_json::Value) -> Self {
        Self {
            rows: vec![row],
            returning: vec![],
        }
    }

    /// Create an insert input with multiple rows.
    pub fn bulk(rows: Vec<serde_json::Value>) -> Self {
        Self {
            rows,
            returning: vec![],
        }
    }

    /// Specify fields to return.
    pub fn with_returning(mut self, fields: Vec<String>) -> Self {
        self.returning = fields;
        self
    }

    /// Check if this is a bulk insert (more than one row).
    pub fn is_bulk(&self) -> bool {
        self.rows.len() > 1
    }
}

impl UpdateInput {
    /// Create an update input.
    pub fn new(filter: FilterOp, set: serde_json::Value) -> Self {
        Self {
            r#where: filter,
            set,
            returning: vec![],
        }
    }

    /// Specify fields to return.
    pub fn with_returning(mut self, fields: Vec<String>) -> Self {
        self.returning = fields;
        self
    }
}

impl DeleteInput {
    /// Create a delete input.
    pub fn new(filter: FilterOp) -> Self {
        Self {
            r#where: filter,
            returning: vec![],
        }
    }

    /// Specify fields to return.
    pub fn with_returning(mut self, fields: Vec<String>) -> Self {
        self.returning = fields;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::data::FilterValue;
    use serde_json::json;

    #[test]
    fn test_insert_single() {
        let input = InsertInput::single(json!({"title": "Test"}))
            .with_returning(vec!["id".into()]);
        
        assert!(!input.is_bulk());
        assert_eq!(input.returning, vec!["id"]);
    }

    #[test]
    fn test_insert_bulk() {
        let input = InsertInput::bulk(vec![
            json!({"title": "A"}),
            json!({"title": "B"}),
        ]);
        
        assert!(input.is_bulk());
    }

    #[test]
    fn test_update_json_roundtrip() {
        let input = UpdateInput::new(
            FilterOp::Eq("cat".into(), FilterValue::String("A".into())),
            json!({"status": "archived"}),
        ).with_returning(vec!["id".into(), "status".into()]);

        let json = serde_json::to_string(&input).unwrap();
        let back: UpdateInput = serde_json::from_str(&json).unwrap();
        assert_eq!(input, back);
    }

    #[test]
    fn test_delete_json_roundtrip() {
        let input = DeleteInput::new(
            FilterOp::Lt("created_at".into(), FilterValue::DateTime(1704067200)),
        ).with_returning(vec!["id".into()]);

        let json = serde_json::to_string(&input).unwrap();
        let back: DeleteInput = serde_json::from_str(&json).unwrap();
        assert_eq!(input, back);
    }
}
