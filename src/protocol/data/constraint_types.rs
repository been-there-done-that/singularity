//! Operation Constraint types for mutation validation.
//!
//! These constraints control **what** a mutation is allowed to modify,
//! independent of **who** (Access Profiles) and **which rows** (RLS).
//!
//! Evaluated at:
//! - PlanAuthorize (compile-time) → 400 on violation
//! - Executor (defense-in-depth) → hard error
//!
//! # Field Presence Definition (IMPORTANT)
//! A field is "in request body" if the key exists in the JSON payload,
//! even if the value is null. Example: `{"role": null}` counts as "set".

use serde::{Deserialize, Serialize};
use crate::protocol::data::FilterOp;

// ============================================================================
// Comparison Operators
// ============================================================================

/// Comparison operator for Length constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    /// Equal (==)
    Eq,
    /// Not equal (!=)
    Ne,
    /// Greater than (>)
    Gt,
    /// Greater than or equal (>=)
    Gte,
    /// Less than (<)
    Lt,
    /// Less than or equal (<=)
    Lte,
}

impl CompareOp {
    /// Evaluate the comparison.
    pub fn evaluate(&self, left: usize, right: usize) -> bool {
        match self {
            Self::Eq => left == right,
            Self::Ne => left != right,
            Self::Gt => left > right,
            Self::Gte => left >= right,
            Self::Lt => left < right,
            Self::Lte => left <= right,
        }
    }
}

// ============================================================================
// Operation Constraints
// ============================================================================

/// Operation constraint for mutation validation.
///
/// These constraints are **additive** — empty list means no extra restrictions.
/// Constraints are evaluated **after** Access Profiles and RLS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationConstraint {
    /// Field must NOT be present in request body.
    /// Applies to: insert, update
    CannotSet {
        field: String,
    },

    /// Field must NOT be changed from existing value.
    /// Applies to: **update ONLY** — invalid for insert.
    CannotChange {
        field: String,
    },

    /// Field MUST be present in request body.
    /// Applies to: insert, update
    MustSet {
        field: String,
    },

    /// Array field must satisfy length constraint.
    /// Applies to: multi-select, relation arrays, file arrays.
    /// If field is missing → treated as length = 0.
    /// If field exists but is not an array → FAIL.
    Length {
        field: String,
        op: CompareOp,
        value: usize,
    },

    /// Each item in array must match predicate.
    /// Predicate is evaluated with the **item as root value**, not the row.
    /// If ANY item fails predicate → FAIL.
    EachItemMatches {
        field: String,
        predicate: FilterOp,
    },
}

impl OperationConstraint {
    /// Returns whether this constraint is valid for the given action.
    pub fn is_valid_for_action(&self, is_update: bool) -> bool {
        match self {
            // CannotChange only applies to updates
            Self::CannotChange { .. } => is_update,
            // All others apply to both insert and update
            _ => true,
        }
    }

    /// Get the field name this constraint applies to.
    pub fn field(&self) -> &str {
        match self {
            Self::CannotSet { field } => field,
            Self::CannotChange { field } => field,
            Self::MustSet { field } => field,
            Self::Length { field, .. } => field,
            Self::EachItemMatches { field, .. } => field,
        }
    }
}

// ============================================================================
// Constraint Error
// ============================================================================

/// Error when a constraint is violated.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintViolation {
    /// The constraint that was violated.
    pub constraint_type: String,
    /// The field involved.
    pub field: String,
    /// Human-readable message.
    pub message: String,
}

impl std::fmt::Display for ConstraintViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", self.constraint_type, self.message, self.field)
    }
}

impl std::error::Error for ConstraintViolation {}

// ============================================================================
// Validation Logic
// ============================================================================

/// Check if a field is "present" in the request body.
/// A field is present if its key exists, even if value is null.
pub fn field_is_present(body: &serde_json::Value, field: &str) -> bool {
    body.get(field).is_some()
}

/// Get array length from a field value.
/// Returns None if field is not an array.
pub fn get_array_length(body: &serde_json::Value, field: &str) -> Result<usize, ConstraintViolation> {
    match body.get(field) {
        None => Ok(0), // Missing = length 0
        Some(serde_json::Value::Array(arr)) => Ok(arr.len()),
        Some(_) => Err(ConstraintViolation {
            constraint_type: "length".into(),
            field: field.into(),
            message: "field is not an array".into(),
        }),
    }
}

/// Validate constraints against a request body (and optionally existing record).
pub fn validate_constraints(
    constraints: &[OperationConstraint],
    body: &serde_json::Value,
    existing: Option<&serde_json::Value>,
    is_update: bool,
) -> Result<(), ConstraintViolation> {
    for constraint in constraints {
        // Check if constraint is valid for this action
        if !constraint.is_valid_for_action(is_update) {
            return Err(ConstraintViolation {
                constraint_type: "invalid_constraint".into(),
                field: constraint.field().into(),
                message: format!("{:?} is not valid for this action", constraint),
            });
        }

        match constraint {
            OperationConstraint::CannotSet { field } => {
                if field_is_present(body, field) {
                    return Err(ConstraintViolation {
                        constraint_type: "cannot_set".into(),
                        field: field.clone(),
                        message: format!("field '{}' cannot be set", field),
                    });
                }
            }

            OperationConstraint::CannotChange { field } => {
                // Only applies to updates
                if let Some(existing) = existing {
                    if field_is_present(body, field) {
                        let new_value = body.get(field);
                        let old_value = existing.get(field);
                        if new_value != old_value {
                            return Err(ConstraintViolation {
                                constraint_type: "cannot_change".into(),
                                field: field.clone(),
                                message: format!("field '{}' cannot be changed", field),
                            });
                        }
                    }
                }
            }

            OperationConstraint::MustSet { field } => {
                if !field_is_present(body, field) {
                    return Err(ConstraintViolation {
                        constraint_type: "must_set".into(),
                        field: field.clone(),
                        message: format!("field '{}' must be set", field),
                    });
                }
            }

            OperationConstraint::Length { field, op, value } => {
                let len = get_array_length(body, field)?;
                if !op.evaluate(len, *value) {
                    return Err(ConstraintViolation {
                        constraint_type: "length".into(),
                        field: field.clone(),
                        message: format!(
                            "field '{}' length {} does not satisfy {:?} {}",
                            field, len, op, value
                        ),
                    });
                }
            }

            OperationConstraint::EachItemMatches { field, predicate } => {
                if let Some(serde_json::Value::Array(items)) = body.get(field) {
                    for (i, item) in items.iter().enumerate() {
                        // Evaluate predicate with item as root value
                        if !evaluate_predicate_on_item(item, predicate) {
                            return Err(ConstraintViolation {
                                constraint_type: "each_item_matches".into(),
                                field: field.clone(),
                                message: format!(
                                    "item {} in '{}' does not match predicate",
                                    i, field
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Evaluate a predicate with the item as the root value.
/// This is a simplified evaluation — the item becomes the context.
fn evaluate_predicate_on_item(item: &serde_json::Value, predicate: &FilterOp) -> bool {
    // For now, support basic predicates on the item value itself
    match predicate {
        FilterOp::Eq(field, value) => {
            let target = if field == "value" || field.is_empty() {
                Some(item)
            } else {
                item.get(field)
            };
            target.map(|v| filter_value_matches(v, value)).unwrap_or(false)
        }
        FilterOp::Like(field, pattern) => {
            let target = if field == "value" || field.is_empty() {
                item.as_str()
            } else {
                item.get(field).and_then(|v| v.as_str())
            };
            target.map(|s| simple_like_match(s, pattern)).unwrap_or(false)
        }
        FilterOp::In(field, values) => {
            let target = if field == "value" || field.is_empty() {
                Some(item)
            } else {
                item.get(field)
            };
            target.map(|v| values.iter().any(|fv| filter_value_matches(v, fv))).unwrap_or(false)
        }
        // For complex predicates, default to true (permissive)
        _ => true,
    }
}

/// Check if a serde_json::Value matches a FilterValue.
fn filter_value_matches(json: &serde_json::Value, filter: &super::FilterValue) -> bool {
    use super::FilterValue;
    match filter {
        FilterValue::Null => json.is_null(),
        FilterValue::Bool(b) => json.as_bool() == Some(*b),
        FilterValue::Int(i) => json.as_i64() == Some(*i),
        FilterValue::Float(f) => json.as_f64() == Some(*f),
        FilterValue::String(s) => json.as_str() == Some(s.as_str()),
        FilterValue::DateTime(ts) => json.as_i64() == Some(*ts),
        FilterValue::Subject => false, // Subject is runtime-resolved, not for item comparison
    }
}

/// Simple SQL LIKE pattern matching (% = wildcard).
fn simple_like_match(s: &str, pattern: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let s = s.to_lowercase();
    
    if pattern.starts_with('%') && pattern.ends_with('%') {
        let middle = &pattern[1..pattern.len()-1];
        s.contains(middle)
    } else if pattern.starts_with('%') {
        s.ends_with(&pattern[1..])
    } else if pattern.ends_with('%') {
        s.starts_with(&pattern[..pattern.len()-1])
    } else {
        s == pattern
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cannot_set_blocks_field() {
        let constraint = OperationConstraint::CannotSet { field: "role".into() };
        let body = json!({"role": "admin"});
        
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().constraint_type, "cannot_set");
    }

    #[test]
    fn test_cannot_set_allows_missing_field() {
        let constraint = OperationConstraint::CannotSet { field: "role".into() };
        let body = json!({"name": "test"});
        
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cannot_set_null_counts_as_set() {
        let constraint = OperationConstraint::CannotSet { field: "role".into() };
        let body = json!({"role": null});
        
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_err()); // null is still "set"
    }

    #[test]
    fn test_cannot_change_blocks_mutation() {
        let constraint = OperationConstraint::CannotChange { field: "role".into() };
        let body = json!({"role": "admin"});
        let existing = json!({"role": "user"});
        
        let result = validate_constraints(&[constraint], &body, Some(&existing), true);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().constraint_type, "cannot_change");
    }

    #[test]
    fn test_cannot_change_allows_same_value() {
        let constraint = OperationConstraint::CannotChange { field: "role".into() };
        let body = json!({"role": "admin"});
        let existing = json!({"role": "admin"});
        
        let result = validate_constraints(&[constraint], &body, Some(&existing), true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cannot_change_invalid_for_insert() {
        let constraint = OperationConstraint::CannotChange { field: "role".into() };
        let body = json!({"role": "admin"});
        
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().constraint_type, "invalid_constraint");
    }

    #[test]
    fn test_must_set_requires_field() {
        let constraint = OperationConstraint::MustSet { field: "title".into() };
        let body = json!({"name": "test"});
        
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().constraint_type, "must_set");
    }

    #[test]
    fn test_must_set_accepts_null() {
        let constraint = OperationConstraint::MustSet { field: "title".into() };
        let body = json!({"title": null});
        
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_ok()); // null counts as "set"
    }

    #[test]
    fn test_length_constraint() {
        let constraint = OperationConstraint::Length {
            field: "tags".into(),
            op: CompareOp::Gte,
            value: 1,
        };
        
        // Empty array fails
        let body = json!({"tags": []});
        let result = validate_constraints(&[constraint.clone()], &body, None, false);
        assert!(result.is_err());
        
        // Non-empty array passes
        let body = json!({"tags": ["foo"]});
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_length_missing_is_zero() {
        let constraint = OperationConstraint::Length {
            field: "tags".into(),
            op: CompareOp::Eq,
            value: 0,
        };
        
        let body = json!({"name": "test"});
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_ok()); // Missing = 0
    }

    #[test]
    fn test_length_non_array_fails() {
        let constraint = OperationConstraint::Length {
            field: "tags".into(),
            op: CompareOp::Gte,
            value: 1,
        };
        
        let body = json!({"tags": "not-an-array"});
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("not an array"));
    }

    #[test]
    fn test_each_item_matches() {
        let constraint = OperationConstraint::EachItemMatches {
            field: "permissions".into(),
            predicate: FilterOp::Like("value".into(), "%read%".into()),
        };
        
        // All items match
        let body = json!({"permissions": ["can_read", "read_only"]});
        let result = validate_constraints(&[constraint.clone()], &body, None, false);
        assert!(result.is_ok());
        
        // One item doesn't match
        let body = json!({"permissions": ["can_read", "write_only"]});
        let result = validate_constraints(&[constraint], &body, None, false);
        assert!(result.is_err());
    }
}
