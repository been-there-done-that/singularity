//! OpRequest - Intent declaration message.
//!
//! This message declares a desired operation but does NOT mutate state.
//! It triggers policy evaluation and may result in a `CapGrant`.

use serde::{Deserialize, Serialize};

use super::opcode::Opcode;
use super::types::{FieldSet, Resource};

/// Operation request - declares intent without executing.
///
/// This is the first message in the capability flow:
/// 1. Client sends `OpRequest` with desired operation
/// 2. Singularity evaluates policy
/// 3. If authorized, returns `CapGrant` with capability token
/// 4. Client uses token in `OpExecute` to perform operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpRequest {
    /// Unique request identifier for correlation.
    pub request_id: String,
    /// Desired operation (namespaced opcode).
    pub op: Opcode,
    /// Target resource.
    pub resource: Resource,
    /// Requested fields (optional, for partial access).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<FieldSet>,
    /// Operation input data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    /// Request timestamp (Unix epoch seconds).
    pub timestamp: u64,
}

impl OpRequest {
    /// Create a new operation request.
    pub fn new(
        request_id: impl Into<String>,
        op: impl Into<Opcode>,
        resource: Resource,
        timestamp: u64,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            op: op.into(),
            resource,
            fields: None,
            input: None,
            timestamp,
        }
    }

    /// Set the requested fields.
    pub fn with_fields(mut self, fields: FieldSet) -> Self {
        self.fields = Some(fields);
        self
    }

    /// Set the input data.
    pub fn with_input(mut self, input: serde_json::Value) -> Self {
        self.input = Some(input);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_op_request_creation() {
        let req = OpRequest::new(
            "req-001",
            "resource.read",
            Resource::instance("user", "123"),
            1704067200,
        );

        assert_eq!(req.request_id, "req-001");
        assert_eq!(req.op.as_str(), "resource.read");
        assert_eq!(req.resource.resource_type, "user");
        assert_eq!(req.resource.resource_id, Some("123".to_string()));
        assert_eq!(req.timestamp, 1704067200);
    }

    #[test]
    fn test_op_request_with_fields() {
        let req = OpRequest::new(
            "req-002",
            "user.profile.read",
            Resource::instance("user", "456"),
            1704067200,
        )
        .with_fields(FieldSet::new(["name", "email"]));

        assert!(req.fields.is_some());
        let fields = req.fields.unwrap();
        assert!(fields.contains("name"));
        assert!(fields.contains("email"));
    }

    #[test]
    fn test_op_request_with_input() {
        let req = OpRequest::new(
            "req-003",
            "resource.create",
            Resource::collection("documents"),
            1704067200,
        )
        .with_input(json!({
            "title": "My Document",
            "content": "Hello, world!"
        }));

        assert!(req.input.is_some());
        let input = req.input.unwrap();
        assert_eq!(input["title"], "My Document");
    }

    #[test]
    fn test_op_request_json_serialization_roundtrip() {
        let req = OpRequest::new(
            "req-004",
            "order.submit",
            Resource::collection("orders"),
            1704067200,
        )
        .with_input(json!({"item_id": "item-789", "quantity": 2}));

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: OpRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }

    #[test]
    fn test_op_request_cbor_serialization_roundtrip() {
        let req = OpRequest::new(
            "req-005",
            "admin.audit.view",
            Resource::collection("audit_logs"),
            1704067200,
        );

        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&req, &mut cbor_bytes).unwrap();
        let deserialized: OpRequest = ciborium::from_reader(&cbor_bytes[..]).unwrap();
        assert_eq!(req, deserialized);
    }
}
