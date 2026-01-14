//! Identity error types.

use thiserror::Error;

/// Errors that can occur during identity verification.
#[derive(Debug, Error)]
pub enum IdentityError {
    /// Token has expired.
    #[error("token expired")]
    TokenExpired,

    /// Token is not yet valid (nbf claim).
    #[error("token not yet valid")]
    TokenNotYetValid,

    /// Invalid signature.
    #[error("invalid signature")]
    InvalidSignature,

    /// Required claim is missing.
    #[error("missing required claim: {0}")]
    MissingClaim(String),

    /// Issuer does not match expected value.
    #[error("issuer mismatch: expected {expected}, got {actual}")]
    IssuerMismatch {
        expected: String,
        actual: String,
    },

    /// Audience does not match expected value.
    #[error("audience mismatch")]
    AudienceMismatch,

    /// Key not found for kid.
    #[error("key not found for kid: {0}")]
    KeyNotFound(String),

    /// Token format is invalid.
    #[error("invalid token format: {0}")]
    InvalidFormat(String),

    /// Algorithm not supported.
    #[error("algorithm not supported: {0}")]
    AlgorithmNotSupported(String),
}

impl IdentityError {
    /// Check if this is an expiration error.
    pub fn is_expired(&self) -> bool {
        matches!(self, Self::TokenExpired)
    }

    /// Check if this is a signature error.
    pub fn is_signature_error(&self) -> bool {
        matches!(self, Self::InvalidSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_error_display() {
        let err = IdentityError::MissingClaim("sub".to_string());
        assert!(err.to_string().contains("sub"));
    }

    #[test]
    fn test_issuer_mismatch_display() {
        let err = IdentityError::IssuerMismatch {
            expected: "https://auth.example.com".to_string(),
            actual: "https://evil.com".to_string(),
        };
        assert!(err.to_string().contains("expected"));
    }
}
