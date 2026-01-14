//! Policy-specific error types.

use thiserror::Error;

/// Errors that can occur during policy evaluation.
#[derive(Debug, Error)]
pub enum PolicyError {
    /// Policy evaluation failed (runtime error in script).
    #[error("policy evaluation failed: {0}")]
    EvaluationFailed(String),

    /// Policy violated sandbox restrictions.
    #[error("sandbox violation: {0}")]
    SandboxViolation(String),

    /// Policy explicitly denied the request.
    #[error("policy denied: {reason}")]
    PolicyDenied {
        reason: String,
    },

    /// Policy script is invalid (syntax error, etc.).
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),

    /// Policy returned non-boolean value.
    #[error("policy must return boolean, got: {0}")]
    NonBooleanResult(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_error_display() {
        let err = PolicyError::PolicyDenied {
            reason: "user is not owner".to_string(),
        };
        assert!(err.to_string().contains("denied"));
        assert!(err.to_string().contains("not owner"));
    }

    #[test]
    fn test_sandbox_violation_display() {
        let err = PolicyError::SandboxViolation("loop detected".to_string());
        assert!(err.to_string().contains("sandbox"));
    }
}
