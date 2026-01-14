//! Execution context - holds verified authority.
//!
//! # Invariant
//!
//! `ExecutionContext` can ONLY be constructed with a `VerifiedCapability`,
//! ensuring no execution path exists without verified authority.
//!
//! # Separation of Concerns
//!
//! - `ExecutionContext` = **authority** (from capability)
//! - `ExecutionMeta` = **observability** (request metadata)
//!
//! This separation prevents request metadata from influencing authorization.

use crate::capability::VerifiedCapability;
use crate::protocol::{FieldSet, Opcode, Resource};

/// Execution context holding verified authority.
///
/// This type can ONLY be constructed with a `VerifiedCapability`,
/// providing a structural guarantee that execution requires authorization.
///
/// The inner capability is private to prevent extraction or bypass.
#[non_exhaustive]
pub struct ExecutionContext {
    capability: VerifiedCapability,
}

impl ExecutionContext {
    /// Create an execution context from a verified capability.
    ///
    /// This is the ONLY way to construct an `ExecutionContext`.
    pub fn new(capability: VerifiedCapability) -> Self {
        Self { capability }
    }

    /// Get the capability ID.
    pub fn cap_id(&self) -> &str {
        self.capability.cap_id()
    }

    /// Get the authorized operation.
    pub fn op(&self) -> &Opcode {
        self.capability.op()
    }

    /// Get the authorized resource.
    pub fn resource(&self) -> &Resource {
        self.capability.resource()
    }

    /// Get the authorized fields.
    pub fn fields(&self) -> &FieldSet {
        self.capability.fields()
    }

    /// Check if a specific field is authorized.
    pub fn is_field_authorized(&self, field: &str) -> bool {
        self.capability.fields().contains(field)
    }

    /// Filter output data to contain only authorized fields.
    ///
    /// For **reads** - call this to mask unauthorized fields.
    pub fn filter_output(&self, data: serde_json::Value) -> serde_json::Value {
        let fields = self.capability.fields();

        // Wildcard allows all fields
        if fields.is_all() {
            return data;
        }

        match data {
            serde_json::Value::Object(map) => {
                let filtered: serde_json::Map<String, serde_json::Value> = map
                    .into_iter()
                    .filter(|(key, _)| fields.contains(key))
                    .collect();
                serde_json::Value::Object(filtered)
            }
            serde_json::Value::Array(arr) => {
                let filtered: Vec<serde_json::Value> =
                    arr.into_iter().map(|v| self.filter_output(v)).collect();
                serde_json::Value::Array(filtered)
            }
            // Non-object values pass through (they have no field structure)
            other => other,
        }
    }

    /// Validate that all fields in write payload are authorized.
    ///
    /// For **writes** - call this to REJECT unauthorized fields (not silently drop).
    pub fn validate_write_fields(
        &self,
        payload: &serde_json::Value,
    ) -> Result<(), super::result::ExecutionError> {
        let fields = self.capability.fields();

        // Wildcard allows all fields
        if fields.is_all() {
            return Ok(());
        }

        if let serde_json::Value::Object(map) = payload {
            for key in map.keys() {
                if !fields.contains(key) {
                    return Err(super::result::ExecutionError::UnauthorizedFieldWrite {
                        field: key.clone(),
                    });
                }
            }
        }

        Ok(())
    }
}

/// Execution metadata for observability.
///
/// This is **NOT** authority - it's request-time context for logging,
/// tracing, and auditing. It must NEVER influence authorization decisions.
#[derive(Debug, Clone, Default)]
pub struct ExecutionMeta {
    /// Request ID for correlation.
    pub request_id: Option<String>,
    /// Client IP for audit logging.
    pub client_ip: Option<String>,
    /// Trace ID for distributed tracing.
    pub trace_id: Option<String>,
}

impl ExecutionMeta {
    /// Create empty metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set request ID.
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }

    /// Set client IP.
    pub fn with_client_ip(mut self, ip: impl Into<String>) -> Self {
        self.client_ip = Some(ip.into());
        self
    }

    /// Set trace ID.
    pub fn with_trace_id(mut self, id: impl Into<String>) -> Self {
        self.trace_id = Some(id.into());
        self
    }
}

/// Explicit execution target (resource being operated on).
///
/// This ensures the resource is passed explicitly to executors,
/// not inferred from payload or capability internals.
#[derive(Debug, Clone)]
pub struct ExecutionTarget {
    /// The resource being operated on.
    pub resource: Resource,
}

impl ExecutionTarget {
    /// Create an execution target.
    pub fn new(resource: Resource) -> Self {
        Self { resource }
    }

    /// Validate that the target matches the authorized resource.
    pub fn validate_against_context(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<(), super::result::ExecutionError> {
        let authorized = ctx.resource();

        // Resource type must match
        if self.resource.resource_type != authorized.resource_type {
            return Err(super::result::ExecutionError::ConstraintViolation {
                constraint: "resource_type".to_string(),
                reason: format!(
                    "expected '{}', got '{}'",
                    authorized.resource_type, self.resource.resource_type
                ),
            });
        }

        // If capability specifies a resource ID, target must match exactly
        if let Some(ref authorized_id) = authorized.resource_id {
            match &self.resource.resource_id {
                Some(target_id) if target_id == authorized_id => Ok(()),
                Some(target_id) => Err(super::result::ExecutionError::ConstraintViolation {
                    constraint: "resource_id".to_string(),
                    reason: format!("expected '{}', got '{}'", authorized_id, target_id),
                }),
                None => Err(super::result::ExecutionError::ConstraintViolation {
                    constraint: "resource_id".to_string(),
                    reason: format!("expected '{}', got collection", authorized_id),
                }),
            }
        } else {
            // Capability is for collection - allow any ID within that collection
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilitySigner, CapabilityVerifier, SigningKey};
    use crate::execution::ExecutionError;
    use crate::protocol::CapabilityPayload;
    use serde_json::json;

    fn create_verified_capability(
        fields: FieldSet,
        resource: Resource,
    ) -> VerifiedCapability {
        let signing_key = SigningKey::generate();
        let signer = CapabilitySigner::new(signing_key);
        let verifier = CapabilityVerifier::new(signer.verifying_key());

        let payload = CapabilityPayload::new(
            "cap-test",
            "resource.read",
            resource,
            fields,
            1704067200,
            1704067260,
        );
        let token = signer.mint(&payload).unwrap();
        verifier.verify(&token, 1704067230).unwrap()
    }

    // ==================== ExecutionContext Tests ====================

    #[test]
    fn test_context_requires_verified_capability() {
        let capability = create_verified_capability(
            FieldSet::new(["name", "email"]),
            Resource::instance("user", "123"),
        );

        let ctx = ExecutionContext::new(capability);
        assert_eq!(ctx.cap_id(), "cap-test");
    }

    #[test]
    fn test_filter_output_masks_unauthorized_fields() {
        let capability = create_verified_capability(
            FieldSet::new(["name", "email"]),
            Resource::instance("user", "123"),
        );

        let ctx = ExecutionContext::new(capability);

        let data = json!({
            "name": "Alice",
            "email": "alice@example.com",
            "password": "secret123",
            "ssn": "123-45-6789"
        });

        let filtered = ctx.filter_output(data);

        assert_eq!(filtered.get("name"), Some(&json!("Alice")));
        assert_eq!(filtered.get("email"), Some(&json!("alice@example.com")));
        assert!(filtered.get("password").is_none());
        assert!(filtered.get("ssn").is_none());
    }

    #[test]
    fn test_filter_output_allows_wildcard() {
        let capability = create_verified_capability(
            FieldSet::all(),
            Resource::instance("user", "123"),
        );

        let ctx = ExecutionContext::new(capability);

        let data = json!({
            "name": "Alice",
            "password": "secret123",
            "anything_else": "allowed"
        });

        let filtered = ctx.filter_output(data.clone());
        assert_eq!(filtered, data);
    }

    #[test]
    fn test_filter_output_handles_arrays() {
        let capability = create_verified_capability(
            FieldSet::new(["name"]),
            Resource::instance("user", "123"),
        );

        let ctx = ExecutionContext::new(capability);

        let data = json!([
            {"name": "Alice", "secret": "x"},
            {"name": "Bob", "secret": "y"}
        ]);

        let filtered = ctx.filter_output(data);

        assert_eq!(filtered[0], json!({"name": "Alice"}));
        assert_eq!(filtered[1], json!({"name": "Bob"}));
    }

    #[test]
    fn test_validate_write_fields_rejects_unauthorized() {
        let capability = create_verified_capability(
            FieldSet::new(["name", "email"]),
            Resource::instance("user", "123"),
        );

        let ctx = ExecutionContext::new(capability);

        let payload = json!({
            "name": "Alice",
            "password": "trying_to_update_password"
        });

        let result = ctx.validate_write_fields(&payload);
        assert!(result.is_err());

        if let Err(ExecutionError::UnauthorizedFieldWrite { field }) = result {
            assert_eq!(field, "password");
        } else {
            panic!("Expected UnauthorizedFieldWrite error");
        }
    }

    #[test]
    fn test_validate_write_fields_allows_authorized() {
        let capability = create_verified_capability(
            FieldSet::new(["name", "email"]),
            Resource::instance("user", "123"),
        );

        let ctx = ExecutionContext::new(capability);

        let payload = json!({
            "name": "Alice",
            "email": "alice@new.com"
        });

        assert!(ctx.validate_write_fields(&payload).is_ok());
    }

    #[test]
    fn test_validate_write_fields_allows_wildcard() {
        let capability = create_verified_capability(
            FieldSet::all(),
            Resource::instance("user", "123"),
        );

        let ctx = ExecutionContext::new(capability);

        let payload = json!({
            "anything": "allowed",
            "password": "even_this"
        });

        assert!(ctx.validate_write_fields(&payload).is_ok());
    }

    // ==================== ExecutionTarget Tests ====================

    #[test]
    fn test_execution_target_matches_exact_resource() {
        let capability = create_verified_capability(
            FieldSet::all(),
            Resource::instance("user", "123"),
        );
        let ctx = ExecutionContext::new(capability);

        let target = ExecutionTarget::new(Resource::instance("user", "123"));
        assert!(target.validate_against_context(&ctx).is_ok());
    }

    #[test]
    fn test_execution_target_rejects_wrong_type() {
        let capability = create_verified_capability(
            FieldSet::all(),
            Resource::instance("user", "123"),
        );
        let ctx = ExecutionContext::new(capability);

        let target = ExecutionTarget::new(Resource::instance("document", "123"));
        assert!(target.validate_against_context(&ctx).is_err());
    }

    #[test]
    fn test_execution_target_rejects_wrong_id() {
        let capability = create_verified_capability(
            FieldSet::all(),
            Resource::instance("user", "123"),
        );
        let ctx = ExecutionContext::new(capability);

        let target = ExecutionTarget::new(Resource::instance("user", "456"));
        assert!(target.validate_against_context(&ctx).is_err());
    }

    #[test]
    fn test_execution_target_collection_allows_any_id() {
        let capability = create_verified_capability(
            FieldSet::all(),
            Resource::collection("users"),
        );
        let ctx = ExecutionContext::new(capability);

        // Collection capability allows any ID
        let target = ExecutionTarget::new(Resource::instance("users", "any-id"));
        assert!(target.validate_against_context(&ctx).is_ok());
    }

    // ==================== ExecutionMeta Tests ====================

    #[test]
    fn test_execution_meta_is_observability_only() {
        let meta = ExecutionMeta::new()
            .with_request_id("req-001")
            .with_client_ip("192.168.1.1")
            .with_trace_id("trace-abc");

        assert_eq!(meta.request_id, Some("req-001".to_string()));
        assert_eq!(meta.client_ip, Some("192.168.1.1".to_string()));
        assert_eq!(meta.trace_id, Some("trace-abc".to_string()));
    }
}
