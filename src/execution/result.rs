//! Execution result types.
//!
//! Separate result types for reads vs writes to prevent data leakage.

use thiserror::Error;

/// Errors that can occur during execution.
#[derive(Debug, Error)]
pub enum ExecutionError {
    /// CAS-style constraint was violated.
    #[error("constraint violation: {constraint} - {reason}")]
    ConstraintViolation {
        constraint: String,
        reason: String,
    },

    /// Attempted to write unauthorized field.
    #[error("unauthorized field in write: '{field}'")]
    UnauthorizedFieldWrite {
        field: String,
    },

    /// Storage backend error.
    #[error("storage error: {0}")]
    StorageError(String),

    /// Operation not supported by executor.
    #[error("operation not supported: {0}")]
    OperationNotSupported(String),

    /// Resource not found.
    #[error("resource not found: {resource_type}/{resource_id}")]
    ResourceNotFound {
        resource_type: String,
        resource_id: String,
    },

    /// Required backend capability missing.
    #[error("backend capability missing: {0}")]
    BackendCapabilityMissing(String),

    /// Target resource does not match capability.
    #[error("resource mismatch: {reason}")]
    ResourceMismatch {
        reason: String,
    },

    /// Bad Request (Input error).
    #[error("bad request: {0}")]
    BadRequest(String),
}

/// Result of an execution operation.
///
/// Separate variants for reads vs writes to prevent accidental data leakage.
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// Read operation result - contains filtered data.
    Read {
        /// Data filtered to authorized fields only.
        data: serde_json::Value,
    },

    /// Write operation result - contains count only, no data.
    Write {
        /// Number of affected rows/records.
        affected_count: u64,
    },

    /// No-op result (e.g., conditional write that didn't apply).
    NoOp,
}

impl ExecutionResult {
    /// Create a read result with the given data.
    pub fn read(data: serde_json::Value) -> Self {
        Self::Read { data }
    }

    /// Create a write result with the given affected count.
    pub fn write(affected_count: u64) -> Self {
        Self::Write { affected_count }
    }

    /// Create a no-op result.
    pub fn no_op() -> Self {
        Self::NoOp
    }

    /// Check if this is a read result.
    pub fn is_read(&self) -> bool {
        matches!(self, Self::Read { .. })
    }

    /// Check if this is a write result.
    pub fn is_write(&self) -> bool {
        matches!(self, Self::Write { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_execution_result_read() {
        let result = ExecutionResult::read(json!({"name": "Alice"}));
        assert!(result.is_read());
        assert!(!result.is_write());
    }

    #[test]
    fn test_execution_result_write() {
        let result = ExecutionResult::write(5);
        assert!(result.is_write());
        assert!(!result.is_read());
    }

    #[test]
    fn test_execution_error_constraint_violation() {
        let err = ExecutionError::ConstraintViolation {
            constraint: "version_eq".to_string(),
            reason: "expected 5, got 6".to_string(),
        };
        assert!(err.to_string().contains("version_eq"));
    }

    #[test]
    fn test_execution_error_unauthorized_field() {
        let err = ExecutionError::UnauthorizedFieldWrite {
            field: "password".to_string(),
        };
        assert!(err.to_string().contains("password"));
        assert!(err.to_string().contains("unauthorized"));
    }
}
