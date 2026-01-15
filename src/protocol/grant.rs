//! CapGrant - Capability grant response.
//!
//! Contains the signed capability token that authorizes execution.

use serde::{Deserialize, Serialize};

use super::opcode::Opcode;
use super::types::{CapabilityBinding, FieldSet, Resource};

/// Internal payload that gets signed.
///
/// This struct is NEVER sent on the wire directly - it's serialized,
/// signed, and encoded into a `CapabilityToken`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityPayload {
    /// Unique capability identifier.
    pub cap_id: String,
    /// Authorized operation.
    pub op: Opcode,
    /// Authorized resource.
    pub resource: Resource,
    /// Authorized fields.
    pub fields: FieldSet,
    /// Issuance timestamp (Unix epoch seconds).
    pub issued_at: u64,
    /// Expiration timestamp (Unix epoch seconds, ≤60s from issued_at).
    pub expires_at: u64,
    /// Optional constraints (JSON object).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<serde_json::Value>,
    /// Optional binding (nonce/IP).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<CapabilityBinding>,
    /// Internal User ID (for ownership attribution).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_user_id: Option<String>,
    /// Hash of (LogicalPlan, PlanGrant) for plan binding.
    /// Executor asserts this matches to prevent mismatched plan/grant reuse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_hash: Option<String>,
    /// Subject roles (e.g. ["admin", "user"]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
}


impl CapabilityPayload {
    /// Create a new capability payload.
    ///
    /// # Panics
    /// Panics if `expires_at <= issued_at`.
    pub fn new(
        cap_id: impl Into<String>,
        op: impl Into<Opcode>,
        resource: Resource,
        fields: FieldSet,
        issued_at: u64,
        expires_at: u64,
    ) -> Self {
        assert!(
            expires_at > issued_at,
            "expires_at must be greater than issued_at"
        );

        Self {
            cap_id: cap_id.into(),
            op: op.into(),
            resource,
            fields,
            issued_at,
            expires_at,
            constraints: None,
            bind: None,
            internal_user_id: None,
            plan_hash: None,
            roles: None,
        }
    }

    /// Set internal user ID for ownership tracking.
    pub fn with_internal_user_id(mut self, internal_user_id: impl Into<String>) -> Self {
        self.internal_user_id = Some(internal_user_id.into());
        self
    }

    /// Set subject roles.
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = Some(roles);
        self
    }


    /// Set plan hash for plan/grant binding.
    /// Executor asserts this matches to prevent mismatched reuse.
    pub fn with_plan_hash(mut self, hash: impl Into<String>) -> Self {
        self.plan_hash = Some(hash.into());
        self
    }

    /// Set constraints on the capability.
    pub fn with_constraints(mut self, constraints: serde_json::Value) -> Self {
        self.constraints = Some(constraints);
        self
    }

    /// Set binding for the capability.
    pub fn with_binding(mut self, binding: CapabilityBinding) -> Self {
        self.bind = Some(binding);
        self
    }

    /// Check if this capability is expired at the given timestamp.
    pub fn is_expired_at(&self, now: u64) -> bool {
        now >= self.expires_at
    }

    /// Serialize the payload to CBOR bytes for signing.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ciborium::ser::Error<std::io::Error>> {
        let mut bytes = Vec::new();
        ciborium::into_writer(self, &mut bytes)?;
        Ok(bytes)
    }

    /// Deserialize the payload from CBOR bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ciborium::de::Error<std::io::Error>> {
        ciborium::from_reader(bytes)
    }
}

/// Opaque signed capability token.
///
/// This is base64-encoded and contains the serialized payload plus signature.
/// The internal format is: `base64(cbor(payload) || signature)`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityToken(String);

impl CapabilityToken {
    /// Create a new capability token from a base64-encoded string.
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// Get the token as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the token and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for CapabilityToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for CapabilityToken {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for CapabilityToken {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// Wire-format response to `OpRequest`.
///
/// Contains the opaque signed token and metadata for the client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapGrant {
    /// Correlates to the originating `OpRequest`.
    pub request_id: String,
    /// Opaque signed capability token.
    pub token: CapabilityToken,
    /// Expiration timestamp (convenience field, also encoded in token).
    pub expires_at: u64,
}

impl CapGrant {
    /// Create a new capability grant.
    pub fn new(
        request_id: impl Into<String>,
        token: CapabilityToken,
        expires_at: u64,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            token,
            expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::opcode::*;
    use serde_json::json;

    #[test]
    fn test_capability_payload_creation() {
        let payload = CapabilityPayload::new(
            "cap-001",
            RESOURCE_READ,
            Resource::instance("user", "123"),
            FieldSet::new(["name", "email"]),
            1704067200,
            1704067260, // 60 seconds later
        );

        assert_eq!(payload.cap_id, "cap-001");
        assert_eq!(payload.op.as_str(), RESOURCE_READ);
        assert!(!payload.is_expired_at(1704067230)); // 30 seconds in
        assert!(payload.is_expired_at(1704067260)); // exactly at expiry
        assert!(payload.is_expired_at(1704067300)); // after expiry
    }

    #[test]
    #[should_panic(expected = "expires_at must be greater than issued_at")]
    fn test_capability_payload_invalid_expiry() {
        CapabilityPayload::new(
            "cap-bad",
            RESOURCE_READ,
            Resource::instance("user", "123"),
            FieldSet::all(),
            1704067260,
            1704067200, // expires before issued - invalid!
        );
    }

    #[test]
    fn test_capability_payload_with_constraints() {
        let payload = CapabilityPayload::new(
            "cap-002",
            RESOURCE_UPDATE,
            Resource::instance("document", "doc-456"),
            FieldSet::new(["content"]),
            1704067200,
            1704067260,
        )
        .with_constraints(json!({"max_size_bytes": 10000}));

        assert!(payload.constraints.is_some());
        assert_eq!(payload.constraints.unwrap()["max_size_bytes"], 10000);
    }

    #[test]
    fn test_capability_payload_with_binding() {
        let payload = CapabilityPayload::new(
            "cap-003",
            RESOURCE_DELETE,
            Resource::instance("file", "file-789"),
            FieldSet::all(),
            1704067200,
            1704067260,
        )
        .with_binding(CapabilityBinding::new(
            Some("nonce-xyz".to_string()),
            Some("192.168.1.100".to_string()),
        ));

        assert!(payload.bind.is_some());
        let bind = payload.bind.unwrap();
        assert_eq!(bind.client_nonce, Some("nonce-xyz".to_string()));
        assert_eq!(bind.ip_hint, Some("192.168.1.100".to_string()));
    }

    #[test]
    fn test_capability_payload_serialization_roundtrip() {
        let payload = CapabilityPayload::new(
            "cap-004",
            "order.submit",
            Resource::collection("orders"),
            FieldSet::all(),
            1704067200,
            1704067260,
        );

        let bytes = payload.to_bytes().unwrap();
        let deserialized = CapabilityPayload::from_bytes(&bytes).unwrap();
        assert_eq!(payload, deserialized);
    }

    #[test]
    fn test_capability_token_opaque() {
        let token = CapabilityToken::new("eyJhbGciOiJFZDI1NTE5In0.base64payload.signature");
        assert_eq!(token.as_str(), "eyJhbGciOiJFZDI1NTE5In0.base64payload.signature");
    }

    #[test]
    fn test_capability_token_from_string() {
        let token: CapabilityToken = "token-string".into();
        assert_eq!(token.as_str(), "token-string");
    }

    #[test]
    fn test_cap_grant_creation() {
        let grant = CapGrant::new(
            "req-001",
            CapabilityToken::new("signed-token-data"),
            1704067260,
        );

        assert_eq!(grant.request_id, "req-001");
        assert_eq!(grant.token.as_str(), "signed-token-data");
        assert_eq!(grant.expires_at, 1704067260);
    }

    #[test]
    fn test_cap_grant_json_serialization_roundtrip() {
        let grant = CapGrant::new(
            "req-002",
            CapabilityToken::new("another-token"),
            1704067260,
        );

        let json = serde_json::to_string(&grant).unwrap();
        let deserialized: CapGrant = serde_json::from_str(&json).unwrap();
        assert_eq!(grant, deserialized);
    }

    #[test]
    fn test_cap_grant_cbor_serialization_roundtrip() {
        let grant = CapGrant::new(
            "req-003",
            CapabilityToken::new("cbor-token"),
            1704067260,
        );

        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&grant, &mut cbor_bytes).unwrap();
        let deserialized: CapGrant = ciborium::from_reader(&cbor_bytes[..]).unwrap();
        assert_eq!(grant, deserialized);
    }
}
