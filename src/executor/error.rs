//! Execution errors.

use std::fmt;

/// Errors during plan execution.
#[derive(Debug, Clone)]
pub enum ExecutionError {
    /// SQL execution failed.
    SqlError {
        message: String,
    },

    /// Row predicate violation (should never happen if Phase C worked correctly).
    PredicateViolation {
        reason: String,
    },

    /// Field mask violation.
    FieldViolation {
        field: String,
        reason: String,
    },

    /// Bulk operation limit exceeded.
    BulkLimitExceeded {
        limit: u32,
        actual: u64,
    },

    /// WHERE clause required but not present.
    WhereRequired {
        operation: String,
    },

    /// Operation not supported.
    UnsupportedOperation {
        reason: String,
    },

    /// Plan/Grant mismatch (defensive check).
    PlanMismatch {
        reason: String,
    },

    /// Internal error.
    InternalError {
        message: String,
    },
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SqlError { message } => write!(f, "SQL error: {}", message),
            Self::PredicateViolation { reason } => {
                write!(f, "row predicate violation: {}", reason)
            }
            Self::FieldViolation { field, reason } => {
                write!(f, "field '{}' violation: {}", field, reason)
            }
            Self::BulkLimitExceeded { limit, actual } => {
                write!(
                    f,
                    "bulk limit exceeded: {} rows affected, limit is {}",
                    actual, limit
                )
            }
            Self::WhereRequired { operation } => {
                write!(f, "{} requires WHERE clause", operation)
            }
            Self::UnsupportedOperation { reason } => {
                write!(f, "unsupported operation: {}", reason)
            }
            Self::PlanMismatch { reason } => {
                write!(f, "plan/grant mismatch: {}", reason)
            }
            Self::InternalError { message } => {
                write!(f, "internal error: {}", message)
            }
        }
    }
}

impl std::error::Error for ExecutionError {}

impl From<rusqlite::Error> for ExecutionError {
    fn from(err: rusqlite::Error) -> Self {
        ExecutionError::SqlError {
            message: err.to_string(),
        }
    }
}
