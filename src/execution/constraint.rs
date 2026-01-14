//! CAS-style constraint validation.
//!
//! Constraints are evaluated BEFORE mutation to ensure TOCTOU safety.
//!
//! # Supported Constraints
//!
//! - `version_eq: N` - Current version must equal N
//! - `not_deleted: true` - Record must not be soft-deleted
//! - `exists: true/false` - Record must/must not exist

use super::result::ExecutionError;

/// Validate CAS-style constraints from capability against current state.
///
/// # Order of Operations (Documented Invariant)
///
/// 1. Resource existence check
/// 2. Constraint validation ← **this function**
/// 3. Field validation
/// 4. Mutation
/// 5. Output filtering
///
/// # Arguments
///
/// * `constraints` - Optional constraint object from capability
/// * `current_state` - Current state of the resource (None if not found)
///
/// # Returns
///
/// `Ok(())` if all constraints pass, `Err` otherwise.
pub fn validate_constraints(
    constraints: Option<&serde_json::Value>,
    current_state: Option<&serde_json::Value>,
) -> Result<(), ExecutionError> {
    let constraints = match constraints {
        Some(c) => c,
        None => return Ok(()), // No constraints to validate
    };

    let constraints_obj = match constraints.as_object() {
        Some(obj) => obj,
        None => {
            return Err(ExecutionError::ConstraintViolation {
                constraint: "format".to_string(),
                reason: "constraints must be an object".to_string(),
            })
        }
    };

    for (key, value) in constraints_obj {
        match key.as_str() {
            "version_eq" => validate_version_eq(value, current_state)?,
            "not_deleted" => validate_not_deleted(value, current_state)?,
            "exists" => validate_exists(value, current_state)?,
            _ => {
                // Unknown constraints are ignored (forward compatibility)
                // Log in production, but don't fail
            }
        }
    }

    Ok(())
}

fn validate_version_eq(
    expected_version: &serde_json::Value,
    current_state: Option<&serde_json::Value>,
) -> Result<(), ExecutionError> {
    let expected = expected_version.as_u64().ok_or_else(|| ExecutionError::ConstraintViolation {
        constraint: "version_eq".to_string(),
        reason: "expected version must be a number".to_string(),
    })?;

    let current_state = current_state.ok_or_else(|| ExecutionError::ConstraintViolation {
        constraint: "version_eq".to_string(),
        reason: "resource not found, cannot check version".to_string(),
    })?;

    let actual = current_state
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ExecutionError::ConstraintViolation {
            constraint: "version_eq".to_string(),
            reason: "resource has no version field".to_string(),
        })?;

    if actual != expected {
        return Err(ExecutionError::ConstraintViolation {
            constraint: "version_eq".to_string(),
            reason: format!("expected version {}, got {}", expected, actual),
        });
    }

    Ok(())
}

fn validate_not_deleted(
    required: &serde_json::Value,
    current_state: Option<&serde_json::Value>,
) -> Result<(), ExecutionError> {
    if !required.as_bool().unwrap_or(false) {
        return Ok(()); // Constraint not active
    }

    let current_state = current_state.ok_or_else(|| ExecutionError::ConstraintViolation {
        constraint: "not_deleted".to_string(),
        reason: "resource not found".to_string(),
    })?;

    let is_deleted = current_state
        .get("deleted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_deleted {
        return Err(ExecutionError::ConstraintViolation {
            constraint: "not_deleted".to_string(),
            reason: "resource is soft-deleted".to_string(),
        });
    }

    Ok(())
}

fn validate_exists(
    should_exist: &serde_json::Value,
    current_state: Option<&serde_json::Value>,
) -> Result<(), ExecutionError> {
    let should_exist = should_exist.as_bool().unwrap_or(true);
    let exists = current_state.is_some();

    if should_exist && !exists {
        return Err(ExecutionError::ConstraintViolation {
            constraint: "exists".to_string(),
            reason: "resource must exist".to_string(),
        });
    }

    if !should_exist && exists {
        return Err(ExecutionError::ConstraintViolation {
            constraint: "exists".to_string(),
            reason: "resource must not exist".to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ==================== version_eq Tests ====================

    #[test]
    fn test_version_eq_passes() {
        let constraints = json!({"version_eq": 5});
        let state = json!({"id": "123", "version": 5});

        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(result.is_ok());
    }

    #[test]
    fn test_version_eq_fails_mismatch() {
        let constraints = json!({"version_eq": 5});
        let state = json!({"id": "123", "version": 6});

        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(matches!(result, Err(ExecutionError::ConstraintViolation { .. })));
    }

    #[test]
    fn test_version_eq_fails_no_state() {
        let constraints = json!({"version_eq": 5});

        let result = validate_constraints(Some(&constraints), None);
        assert!(matches!(result, Err(ExecutionError::ConstraintViolation { .. })));
    }

    #[test]
    fn test_version_eq_fails_no_version_field() {
        let constraints = json!({"version_eq": 5});
        let state = json!({"id": "123", "name": "test"});

        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(matches!(result, Err(ExecutionError::ConstraintViolation { .. })));
    }

    // ==================== not_deleted Tests ====================

    #[test]
    fn test_not_deleted_passes() {
        let constraints = json!({"not_deleted": true});
        let state = json!({"id": "123", "deleted": false});

        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(result.is_ok());
    }

    #[test]
    fn test_not_deleted_passes_no_deleted_field() {
        let constraints = json!({"not_deleted": true});
        let state = json!({"id": "123", "name": "test"});

        // No deleted field means not deleted
        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(result.is_ok());
    }

    #[test]
    fn test_not_deleted_fails() {
        let constraints = json!({"not_deleted": true});
        let state = json!({"id": "123", "deleted": true});

        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(matches!(result, Err(ExecutionError::ConstraintViolation { .. })));
    }

    // ==================== exists Tests ====================

    #[test]
    fn test_exists_true_passes() {
        let constraints = json!({"exists": true});
        let state = json!({"id": "123"});

        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(result.is_ok());
    }

    #[test]
    fn test_exists_true_fails() {
        let constraints = json!({"exists": true});

        let result = validate_constraints(Some(&constraints), None);
        assert!(matches!(result, Err(ExecutionError::ConstraintViolation { .. })));
    }

    #[test]
    fn test_exists_false_passes() {
        let constraints = json!({"exists": false});

        let result = validate_constraints(Some(&constraints), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_exists_false_fails() {
        let constraints = json!({"exists": false});
        let state = json!({"id": "123"});

        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(matches!(result, Err(ExecutionError::ConstraintViolation { .. })));
    }

    // ==================== No Constraints Tests ====================

    #[test]
    fn test_no_constraints_passes() {
        let result = validate_constraints(None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_constraints_passes() {
        let constraints = json!({});
        let result = validate_constraints(Some(&constraints), None);
        assert!(result.is_ok());
    }

    // ==================== Combined Constraints ====================

    #[test]
    fn test_multiple_constraints_all_pass() {
        let constraints = json!({
            "version_eq": 3,
            "not_deleted": true,
            "exists": true
        });
        let state = json!({
            "id": "123",
            "version": 3,
            "deleted": false
        });

        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_constraints_one_fails() {
        let constraints = json!({
            "version_eq": 3,
            "not_deleted": true
        });
        let state = json!({
            "id": "123",
            "version": 4,  // Wrong version
            "deleted": false
        });

        let result = validate_constraints(Some(&constraints), Some(&state));
        assert!(matches!(result, Err(ExecutionError::ConstraintViolation { .. })));
    }
}
