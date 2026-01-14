//! Capability-specific error types.
//!
//! All errors are semantic and distinguishable for audit logging.
//! No error variant leaks sensitive material (keys, signatures).

use crate::protocol::Resource;
use thiserror::Error;

/// Maximum allowed TTL for capabilities (60 seconds).
pub const MAX_CAP_TTL_SECS: u64 = 60;

/// Maximum allowed clock skew tolerance (5 seconds).
pub const MAX_CLOCK_SKEW_SECS: u64 = 5;

/// Errors that can occur during capability operations.
#[derive(Debug, Error)]
pub enum CapabilityError {
    /// Capability has expired.
    #[error("capability {cap_id} expired at {expired_at}, current time is {now}")]
    Expired {
        cap_id: String,
        expired_at: u64,
        now: u64,
    },

    /// Token was issued in the future (clock skew or manipulation).
    #[error("capability issued at {issued_at} is in the future (current time: {now})")]
    ClockSkew {
        now: u64,
        issued_at: u64,
    },

    /// TTL exceeds maximum allowed duration.
    #[error("capability TTL {ttl_secs}s exceeds maximum allowed {max_ttl_secs}s")]
    InvalidTTL {
        ttl_secs: u64,
        max_ttl_secs: u64,
    },

    /// Cryptographic signature verification failed.
    #[error("invalid signature")]
    InvalidSignature,

    /// Token format is malformed (cannot be decoded).
    #[error("malformed token: {0}")]
    TokenMalformed(String),

    /// Operation scope does not match capability.
    #[error("scope mismatch: expected '{expected}', got '{actual}'")]
    ScopeMismatch {
        expected: String,
        actual: String,
    },

    /// Resource does not match capability.
    #[error("resource mismatch: expected {expected:?}, got {actual:?}")]
    ResourceMismatch {
        expected: Resource,
        actual: Resource,
    },

    /// Requested field is not authorized by capability.
    #[error("field '{unauthorized_field}' is not authorized")]
    FieldMismatch {
        unauthorized_field: String,
    },

    /// Client binding (nonce or IP) does not match.
    #[error("binding mismatch: {reason}")]
    BindingMismatch {
        reason: String,
    },

    /// CBOR encoding/decoding error.
    #[error("encoding error: {0}")]
    EncodingError(String),

    /// Key material error.
    #[error("key error: {0}")]
    KeyError(String),
}

impl CapabilityError {
    /// Check if this error indicates a potential attack (vs. benign failure).
    pub fn is_security_relevant(&self) -> bool {
        matches!(
            self,
            CapabilityError::InvalidSignature
                | CapabilityError::ScopeMismatch { .. }
                | CapabilityError::ResourceMismatch { .. }
                | CapabilityError::FieldMismatch { .. }
                | CapabilityError::BindingMismatch { .. }
                | CapabilityError::ClockSkew { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_expired() {
        let err = CapabilityError::Expired {
            cap_id: "cap-001".to_string(),
            expired_at: 1704067260,
            now: 1704067300,
        };
        assert!(err.to_string().contains("expired"));
        assert!(err.to_string().contains("cap-001"));
    }

    #[test]
    fn test_error_display_clock_skew() {
        let err = CapabilityError::ClockSkew {
            now: 1704067200,
            issued_at: 1704067300,
        };
        assert!(err.to_string().contains("future"));
    }

    #[test]
    fn test_error_display_invalid_ttl() {
        let err = CapabilityError::InvalidTTL {
            ttl_secs: 120,
            max_ttl_secs: 60,
        };
        assert!(err.to_string().contains("exceeds"));
    }

    #[test]
    fn test_security_relevant_errors() {
        assert!(CapabilityError::InvalidSignature.is_security_relevant());
        assert!(CapabilityError::BindingMismatch {
            reason: "nonce".to_string()
        }.is_security_relevant());
        
        // Expired is benign (normal lifecycle)
        assert!(!CapabilityError::Expired {
            cap_id: "x".to_string(),
            expired_at: 0,
            now: 1,
        }.is_security_relevant());
        
        // Encoding error is benign (malformed input)
        assert!(!CapabilityError::EncodingError("bad".to_string()).is_security_relevant());
    }

    #[test]
    fn test_max_cap_ttl_is_60_seconds() {
        assert_eq!(MAX_CAP_TTL_SECS, 60);
    }
}
