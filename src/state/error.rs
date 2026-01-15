//! State error types.

use thiserror::Error;

/// Errors that can occur during state operations.
#[derive(Debug, Error)]
pub enum StateError {
    /// Resource not found.
    #[error("not found: {resource_type}/{resource_id}")]
    NotFound {
        resource_type: String,
        resource_id: String,
    },

    /// CAS-style constraint was violated.
    #[error("{reason}")]
    ConstraintViolation {
        constraint: String,
        reason: String,
    },

    /// Required capability not supported by backend.
    #[error("capability not supported: {0}")]
    CapabilityNotSupported(String),

    /// Connection or I/O error.
    #[error("connection error: {0}")]
    ConnectionError(String),

    /// Internal backend error.
    #[error("internal error: {0}")]
    InternalError(String),

    /// Bad Request (Input error).
    #[error("bad request: {0}")]
    BadRequest(String),
}

impl StateError {
    /// Check if this is a "not found" error.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }

    /// Check if this is a constraint violation.
    pub fn is_constraint_violation(&self) -> bool {
        matches!(self, Self::ConstraintViolation { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_error_display() {
        let err = StateError::NotFound {
            resource_type: "user".to_string(),
            resource_id: "123".to_string(),
        };
        assert!(err.to_string().contains("user"));
        assert!(err.to_string().contains("123"));
        assert!(err.is_not_found());
    }

    #[test]
    fn test_constraint_violation_display() {
        let err = StateError::ConstraintViolation {
            constraint: "version_eq".to_string(),
            reason: "expected 5, got 6".to_string(),
        };
        assert_eq!(err.to_string(), "expected 5, got 6");
        assert!(err.is_constraint_violation());
    }
}
