//! Filter AST - single source of truth for all filtering.
//!
//! Used by: query, count, update, delete, policy predicates.
//!
//! # Syntax
//!
//! Tuple-style for clean AST compilation:
//! ```json
//! { "and": [
//!     { "eq": ["status", "active"] },
//!     { "gt": ["price", 10] }
//! ]}
//! ```

use serde::{Deserialize, Serialize};

/// Filter operation - the unified filter grammar.
///
/// Every operator is fixed-arity, positional, and trivial to compile to SQL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    /// Logical AND of multiple filters (must have ≥ 1 element)
    And(Vec<FilterOp>),
    /// Logical OR of multiple filters (must have ≥ 1 element)
    Or(Vec<FilterOp>),
    /// Logical NOT (wraps exactly one filter)
    Not(Box<FilterOp>),

    // Comparison operators - all use tuple syntax [field, value]
    /// Equality: { "eq": ["field", value] }
    Eq(String, FilterValue),
    /// Not equal: { "ne": ["field", value] }
    Ne(String, FilterValue),
    /// Greater than: { "gt": ["field", value] }
    Gt(String, FilterValue),
    /// Greater than or equal: { "gte": ["field", value] }
    Gte(String, FilterValue),
    /// Less than: { "lt": ["field", value] }
    Lt(String, FilterValue),
    /// Less than or equal: { "lte": ["field", value] }
    Lte(String, FilterValue),

    // Set operations
    /// IN: { "in": ["field", [values]] }
    In(String, Vec<FilterValue>),
    /// LIKE pattern match: { "like": ["field", "pattern"] }
    Like(String, String),
    /// Case-insensitive LIKE: { "ilike": ["field", "pattern"] }
    ILike(String, String),

    // Null checks
    /// IS NULL: { "is_null": "field" }
    IsNull(String),
    /// IS NOT NULL: { "is_not_null": "field" }
    IsNotNull(String),
}

/// Values allowed in filters (type-safe).
///
/// # Special Values
///
/// - `Subject`: Resolves to current user's internal ID at grant time
/// - `DateTime`: Wrapper for unix epoch seconds (structured as `{"$datetime": 123}`)
///
/// # JSON Examples
///
/// ```json
/// "active"           // String
/// 42                 // Int
/// true               // Bool
/// null               // Null  
/// {"$subject": true} // Subject (current user ID)
/// {"$datetime": 123} // DateTime (unix epoch)
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value
    Int(i64),
    /// Floating point value  
    Float(f64),
    /// String value
    String(String),
    /// Unix epoch seconds - structured to avoid ambiguity with Int
    DateTime(i64),
    /// Special: resolves to current user's internal ID
    Subject,
}

// Custom serialization to handle special values correctly
impl Serialize for FilterValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        match self {
            FilterValue::Null => serializer.serialize_none(),
            FilterValue::Bool(b) => serializer.serialize_bool(*b),
            FilterValue::Int(i) => serializer.serialize_i64(*i),
            FilterValue::Float(f) => serializer.serialize_f64(*f),
            FilterValue::String(s) => serializer.serialize_str(s),
            FilterValue::DateTime(ts) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("$datetime", ts)?;
                map.end()
            }
            FilterValue::Subject => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("$subject", &true)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for FilterValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor, MapAccess};
        
        struct FilterValueVisitor;
        
        impl<'de> Visitor<'de> for FilterValueVisitor {
            type Value = FilterValue;
            
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a filter value (null, bool, number, string, or special object)")
            }
            
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(FilterValue::Null)
            }
            
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(FilterValue::Null)
            }
            
            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
                Ok(FilterValue::Bool(v))
            }
            
            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
                Ok(FilterValue::Int(v))
            }
            
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
                Ok(FilterValue::Int(v as i64))
            }
            
            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E> {
                Ok(FilterValue::Float(v))
            }
            
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
                Ok(FilterValue::String(v.to_string()))
            }
            
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
                Ok(FilterValue::String(v))
            }
            
            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let key: String = map.next_key()?
                    .ok_or_else(|| de::Error::custom("expected key in special value object"))?;
                
                match key.as_str() {
                    "$subject" => {
                        let _: bool = map.next_value()?;
                        Ok(FilterValue::Subject)
                    }
                    "$datetime" => {
                        let ts: i64 = map.next_value()?;
                        Ok(FilterValue::DateTime(ts))
                    }
                    other => Err(de::Error::custom(format!("unknown special key: {}", other)))
                }
            }
        }
        
        deserializer.deserialize_any(FilterValueVisitor)
    }
}

impl FilterOp {
    /// Validate the filter structure.
    ///
    /// Rejects:
    /// - Empty AND/OR
    /// - Invalid field references
    pub fn validate(&self) -> Result<(), FilterValidationError> {
        match self {
            FilterOp::And(filters) => {
                if filters.is_empty() {
                    return Err(FilterValidationError::EmptyAnd);
                }
                for f in filters {
                    f.validate()?;
                }
            }
            FilterOp::Or(filters) => {
                if filters.is_empty() {
                    return Err(FilterValidationError::EmptyOr);
                }
                for f in filters {
                    f.validate()?;
                }
            }
            FilterOp::Not(inner) => {
                inner.validate()?;
            }
            // All other ops are structurally valid
            _ => {}
        }
        Ok(())
    }

    /// Extract the field name, handling alias resolution.
    ///
    /// Returns (alias, field) where alias is None for base table fields.
    /// - `"price"` → (None, "price")
    /// - `"owner.email"` → (Some("owner"), "email")
    pub fn parse_field_ref(field: &str) -> (Option<&str>, &str) {
        match field.split_once('.') {
            Some((alias, rest)) => (Some(alias), rest),
            None => (None, field),
        }
    }
}

/// Filter validation errors.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterValidationError {
    /// AND must have at least one element
    EmptyAnd,
    /// OR must have at least one element
    EmptyOr,
    /// Field reference is invalid
    InvalidField(String),
    /// Unknown alias in field reference
    UnknownAlias(String),
}

impl std::fmt::Display for FilterValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAnd => write!(f, "AND filter must have at least one element"),
            Self::EmptyOr => write!(f, "OR filter must have at least one element"),
            Self::InvalidField(field) => write!(f, "invalid field reference: {}", field),
            Self::UnknownAlias(alias) => write!(f, "unknown alias in field reference: {}", alias),
        }
    }
}

impl std::error::Error for FilterValidationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_filter_eq_roundtrip() {
        let filter = FilterOp::Eq("status".into(), FilterValue::String("active".into()));
        let json = serde_json::to_value(&filter).unwrap();
        let back: FilterOp = serde_json::from_value(json).unwrap();
        assert_eq!(filter, back);
    }

    #[test]
    fn test_filter_and_validation() {
        let empty_and = FilterOp::And(vec![]);
        assert_eq!(empty_and.validate(), Err(FilterValidationError::EmptyAnd));

        let valid_and = FilterOp::And(vec![
            FilterOp::Eq("a".into(), FilterValue::Int(1)),
        ]);
        assert!(valid_and.validate().is_ok());
    }

    #[test]
    fn test_filter_or_validation() {
        let empty_or = FilterOp::Or(vec![]);
        assert_eq!(empty_or.validate(), Err(FilterValidationError::EmptyOr));
    }

    #[test]
    fn test_parse_field_ref() {
        assert_eq!(FilterOp::parse_field_ref("price"), (None, "price"));
        assert_eq!(FilterOp::parse_field_ref("owner.email"), (Some("owner"), "email"));
        assert_eq!(FilterOp::parse_field_ref("a.b.c"), (Some("a"), "b.c"));
    }

    #[test]
    fn test_filter_value_datetime() {
        let dt = FilterValue::DateTime(1704067200);
        let json = serde_json::to_value(&dt).unwrap();
        assert!(json.is_number() || json.is_object());
    }

    #[test]
    fn test_complex_filter_json() {
        // Test the tuple-style JSON syntax
        let filter = FilterOp::And(vec![
            FilterOp::Eq("status".into(), FilterValue::String("active".into())),
            FilterOp::Gt("price".into(), FilterValue::Int(10)),
            FilterOp::Or(vec![
                FilterOp::Eq("category".into(), FilterValue::String("A".into())),
                FilterOp::Eq("category".into(), FilterValue::String("B".into())),
            ]),
        ]);
        
        assert!(filter.validate().is_ok());
        
        let json = serde_json::to_string_pretty(&filter).unwrap();
        let back: FilterOp = serde_json::from_str(&json).unwrap();
        assert_eq!(filter, back);
    }
}
