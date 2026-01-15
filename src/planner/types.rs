//! Core planner types — resolved references.
//!
//! No raw strings past this point.

use serde::{Deserialize, Serialize};

/// Resolved model reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    /// Model name (table name).
    pub name: String,
    /// Model ID from __models.
    pub id: String,
}

impl ModelRef {
    /// Create a new model reference.
    pub fn new(name: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: id.into(),
        }
    }
}

/// Resolved field reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldRef {
    /// The model this field belongs to.
    pub model: ModelRef,
    /// Field name.
    pub name: String,
    /// Field type.
    pub field_type: FieldType,
}

impl FieldRef {
    /// Create a new field reference.
    pub fn new(model: ModelRef, name: impl Into<String>, field_type: FieldType) -> Self {
        Self {
            model,
            name: name.into(),
            field_type,
        }
    }
}

/// Simplified field types for planning.
///
/// This matches the kernel type system, not SQL types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Int,
    Float,
    Bool,
    Json,
    /// Timestamp (stored as unix epoch)
    Timestamp,
    /// Unknown/unresolved type
    Unknown,
}

impl Default for FieldType {
    fn default() -> Self {
        Self::Unknown
    }
}

impl FieldType {
    /// Parse from the __fields.field_type JSON.
    pub fn from_schema_json(json: &serde_json::Value) -> Self {
        let type_str = json
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");

        match type_str.to_lowercase().as_str() {
            "string" | "text" => Self::String,
            "int" | "integer" => Self::Int,
            "float" | "number" | "real" => Self::Float,
            "bool" | "boolean" => Self::Bool,
            "json" | "jsonb" => Self::Json,
            "timestamp" | "timestamptz" | "datetime" => Self::Timestamp,
            _ => Self::Unknown,
        }
    }
}

/// Resolved relation reference (FK join).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationRef {
    /// Relation name (e.g., "items.owner").
    pub name: String,
    /// Source model.
    pub from_model: ModelRef,
    /// Source field (the FK column).
    pub from_field: FieldRef,
    /// Target model.
    pub to_model: ModelRef,
    /// Target field (usually the PK).
    pub to_field: FieldRef,
}

impl RelationRef {
    /// Create a new relation reference.
    pub fn new(
        name: impl Into<String>,
        from_model: ModelRef,
        from_field: FieldRef,
        to_model: ModelRef,
        to_field: FieldRef,
    ) -> Self {
        Self {
            name: name.into(),
            from_model,
            from_field,
            to_model,
            to_field,
        }
    }
}

/// A literal row for insert operations.
///
/// Values are still JSON at this stage, but the structure is validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowLiteral {
    /// Row data as JSON object.
    pub data: serde_json::Value,
}

impl RowLiteral {
    /// Create a new row literal.
    pub fn new(data: serde_json::Value) -> Self {
        Self { data }
    }

    /// Get a field value from the row.
    pub fn get(&self, field: &str) -> Option<&serde_json::Value> {
        self.data.get(field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_field_type_parsing() {
        assert_eq!(
            FieldType::from_schema_json(&json!({"type": "String"})),
            FieldType::String
        );
        assert_eq!(
            FieldType::from_schema_json(&json!({"type": "Int"})),
            FieldType::Int
        );
        assert_eq!(
            FieldType::from_schema_json(&json!({"type": "timestamp"})),
            FieldType::Timestamp
        );
        assert_eq!(
            FieldType::from_schema_json(&json!({"type": "weird"})),
            FieldType::Unknown
        );
    }

    #[test]
    fn test_model_ref() {
        let model = ModelRef::new("items", "uuid-123");
        assert_eq!(model.name, "items");
        assert_eq!(model.id, "uuid-123");
    }

    #[test]
    fn test_row_literal() {
        let row = RowLiteral::new(json!({"name": "test", "price": 42}));
        assert_eq!(row.get("name"), Some(&json!("test")));
        assert_eq!(row.get("price"), Some(&json!(42)));
        assert_eq!(row.get("missing"), None);
    }
}
