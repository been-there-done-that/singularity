//! OpExecute - Execution request message.
//!
//! This is the ONLY message type that can mutate state.
//! It MUST include a valid capability token.

use serde::{Deserialize, Serialize};

use super::grant::CapabilityToken;

/// Execution request - performs the authorized operation.
///
/// This is the final message in the capability flow:
/// 1. Client received `CapGrant` with capability token
/// 2. Client sends `OpExecute` with the token
/// 3. Singularity verifies the token
/// 4. If valid, executes the operation
/// 5. Returns result or error
///
/// # Security Invariant
/// Execution MUST fail if:
/// - Token is expired
/// - Signature is invalid
/// - Scope mismatch
/// - Resource mismatch
/// - Field mismatch
/// - Binding mismatch (if binding was specified)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpExecute {
    /// Unique execution request identifier.
    pub execute_id: String,
    /// Opaque capability token authorizing this execution.
    pub token: CapabilityToken,
    /// Execution payload (must match capability scope).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    /// Request timestamp (Unix epoch seconds).
    pub timestamp: u64,
}

impl OpExecute {
    /// Create a new execution request.
    pub fn new(
        execute_id: impl Into<String>,
        token: CapabilityToken,
        timestamp: u64,
    ) -> Self {
        Self {
            execute_id: execute_id.into(),
            token,
            payload: None,
            timestamp,
        }
    }

    /// Set the execution payload.
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_op_execute_creation() {
        let exec = OpExecute::new(
            "exec-001",
            CapabilityToken::new("valid-token-here"),
            1704067230,
        );

        assert_eq!(exec.execute_id, "exec-001");
        assert_eq!(exec.token.as_str(), "valid-token-here");
        assert_eq!(exec.timestamp, 1704067230);
        assert!(exec.payload.is_none());
    }

    #[test]
    fn test_op_execute_with_payload() {
        let exec = OpExecute::new(
            "exec-002",
            CapabilityToken::new("another-valid-token"),
            1704067230,
        )
        .with_payload(json!({
            "title": "Updated Title",
            "content": "New content here"
        }));

        assert!(exec.payload.is_some());
        let payload = exec.payload.unwrap();
        assert_eq!(payload["title"], "Updated Title");
    }

    #[test]
    fn test_op_execute_json_serialization_roundtrip() {
        let exec = OpExecute::new(
            "exec-003",
            CapabilityToken::new("token-for-json-test"),
            1704067230,
        )
        .with_payload(json!({"action": "confirm", "amount": 99.99}));

        let json = serde_json::to_string(&exec).unwrap();
        let deserialized: OpExecute = serde_json::from_str(&json).unwrap();
        assert_eq!(exec, deserialized);
    }

    #[test]
    fn test_op_execute_cbor_serialization_roundtrip() {
        let exec = OpExecute::new(
            "exec-004",
            CapabilityToken::new("token-for-cbor-test"),
            1704067230,
        );

        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&exec, &mut cbor_bytes).unwrap();
        let deserialized: OpExecute = ciborium::from_reader(&cbor_bytes[..]).unwrap();
        assert_eq!(exec, deserialized);
    }

    #[test]
    fn test_op_execute_token_is_opaque() {
        // The token is just a string - OpExecute doesn't parse it
        // Parsing and verification happens in the execution layer
        let opaque_token = "this-could-be-anything-the-execute-doesnt-care";
        let exec = OpExecute::new("exec-005", CapabilityToken::new(opaque_token), 1704067230);

        // We can only access it as a string
        assert_eq!(exec.token.as_str(), opaque_token);
    }
}
