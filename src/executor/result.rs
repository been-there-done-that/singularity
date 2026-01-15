//! Execution result types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of plan execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExecutionResult {
    /// Query result: rows of data.
    Rows(Vec<Row>),

    /// Count result.
    Count(u64),

    /// Mutation result: affected rows and optional returning data.
    Affected {
        /// Number of rows affected.
        rows: u64,
        /// Rows returned via RETURNING clause.
        returning: Vec<Row>,
    },
}

impl ExecutionResult {
    /// Create an empty rows result.
    pub fn empty() -> Self {
        Self::Rows(vec![])
    }

    /// Create a count result.
    pub fn count(n: u64) -> Self {
        Self::Count(n)
    }

    /// Create an affected result.
    pub fn affected(rows: u64) -> Self {
        Self::Affected {
            rows,
            returning: vec![],
        }
    }

    /// Create an affected result with returning data.
    pub fn affected_with_returning(rows: u64, returning: Vec<Row>) -> Self {
        Self::Affected { rows, returning }
    }

    /// Get rows if this is a Rows result.
    pub fn rows(&self) -> Option<&[Row]> {
        match self {
            Self::Rows(rows) => Some(rows),
            _ => None,
        }
    }

    /// Get count if this is a Count result.
    pub fn count_value(&self) -> Option<u64> {
        match self {
            Self::Count(n) => Some(*n),
            _ => None,
        }
    }
}

/// A single row of data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// Column values keyed by column name.
    pub columns: HashMap<String, serde_json::Value>,
}

impl Row {
    /// Create a new empty row.
    pub fn new() -> Self {
        Self {
            columns: HashMap::new(),
        }
    }

    /// Create a row from a HashMap.
    pub fn from_map(columns: HashMap<String, serde_json::Value>) -> Self {
        Self { columns }
    }

    /// Get a column value.
    pub fn get(&self, column: &str) -> Option<&serde_json::Value> {
        self.columns.get(column)
    }

    /// Set a column value.
    pub fn set(&mut self, column: impl Into<String>, value: serde_json::Value) {
        self.columns.insert(column.into(), value);
    }

    /// Get string value.
    pub fn get_str(&self, column: &str) -> Option<&str> {
        self.get(column).and_then(|v| v.as_str())
    }

    /// Get i64 value.
    pub fn get_i64(&self, column: &str) -> Option<i64> {
        self.get(column).and_then(|v| v.as_i64())
    }
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl From<serde_json::Value> for Row {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Object(obj) => {
                let columns: HashMap<String, serde_json::Value> =
                    obj.into_iter().collect();
                Self { columns }
            }
            _ => Self::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_row_get_set() {
        let mut row = Row::new();
        row.set("id", json!("123"));
        row.set("count", json!(42));

        assert_eq!(row.get_str("id"), Some("123"));
        assert_eq!(row.get_i64("count"), Some(42));
    }

    #[test]
    fn test_execution_result_rows() {
        let result = ExecutionResult::Rows(vec![Row::new()]);
        assert!(result.rows().is_some());
        assert!(result.count_value().is_none());
    }

    #[test]
    fn test_execution_result_count() {
        let result = ExecutionResult::count(42);
        assert!(result.rows().is_none());
        assert_eq!(result.count_value(), Some(42));
    }
}
