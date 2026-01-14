//! Policy context - read-only inputs exposed to policy scripts.
//!
//! # Design
//!
//! - `PolicySubject` is a **typed, curated view** of identity (not raw JSON)
//! - `PolicyContext` is read-only and deterministic
//! - No access to time except via `env.now`

use crate::protocol::{Opcode, Resource};
use std::collections::HashMap;

/// Typed, curated view of the authenticated subject.
///
/// This is NOT raw JWT claims - it's a stable, documented structure
/// that policy authors can rely on.
#[derive(Debug, Clone)]
pub struct PolicySubject {
    /// Unique subject identifier (e.g., user ID).
    pub id: String,
    /// Internal persistend ID (managed by state).
    pub internal_id: Option<String>,
    /// Subject's roles (e.g., ["admin", "editor"]).
    pub roles: Vec<String>,
    /// Optional namespaced claims for extension.
    pub claims: HashMap<String, serde_json::Value>,
}

impl PolicySubject {
    /// Create a new policy subject.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            internal_id: None,
            roles: Vec::new(),
            claims: HashMap::new(),
        }
    }

    /// Add roles to the subject.
    pub fn with_roles(mut self, roles: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.roles = roles.into_iter().map(Into::into).collect();
        self
    }

    /// Add a claim to the subject.
    pub fn with_claim(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.claims.insert(key.into(), value);
        self
    }

    /// Check if subject has a specific role.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Set the internal ID for the subject.
    pub fn with_internal_id(mut self, internal_id: impl Into<String>) -> Self {
        self.internal_id = Some(internal_id.into());
        self
    }
}

/// Environment variables for policy evaluation.
///
/// This is the ONLY source of time and environment data for policies,
/// ensuring determinism.
#[derive(Debug, Clone)]
pub struct PolicyEnv {
    /// Current timestamp (Unix epoch seconds).
    pub now: u64,
    /// Optional environment-specific data.
    pub vars: HashMap<String, serde_json::Value>,
}

impl PolicyEnv {
    /// Create a new policy environment with the given timestamp.
    pub fn new(now: u64) -> Self {
        Self {
            now,
            vars: HashMap::new(),
        }
    }

    /// Add an environment variable.
    pub fn with_var(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.vars.insert(key.into(), value);
        self
    }
}

/// Read-only context exposed to policy scripts.
///
/// All fields are immutable from the policy's perspective.
/// Policies CANNOT modify any of these values.
#[derive(Debug, Clone)]
pub struct PolicyContext {
    /// Authenticated subject (typed, not raw JSON).
    pub subject: PolicySubject,
    /// Target resource for the operation.
    pub resource: Resource,
    /// Requested operation.
    pub op: Opcode,
    /// Request input/payload (optional).
    pub input: Option<serde_json::Value>,
    /// Resource owner ID (if known).
    pub resource_owner: Option<String>,
    /// Environment (time, etc.) - only source of non-deterministic data.
    pub env: PolicyEnv,
}

impl PolicyContext {
    /// Create a new policy context.
    pub fn new(
        subject: PolicySubject,
        resource: Resource,
        op: impl Into<Opcode>,
        env: PolicyEnv,
    ) -> Self {
        Self {
            subject,
            resource,
            op: op.into(),
            input: None,
            resource_owner: None,
            env,
        }
    }

    /// Set the input payload.
    pub fn with_input(mut self, input: serde_json::Value) -> Self {
        self.input = Some(input);
        self
    }

    /// Set the resource owner ID.
    pub fn with_resource_owner(mut self, owner_id: impl Into<String>) -> Self {
        self.resource_owner = Some(owner_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_subject_creation() {
        let subject = PolicySubject::new("user-123")
            .with_roles(["admin", "editor"]);

        assert_eq!(subject.id, "user-123");
        assert!(subject.has_role("admin"));
        assert!(subject.has_role("editor"));
        assert!(!subject.has_role("guest"));
    }

    #[test]
    fn test_policy_subject_with_claims() {
        let subject = PolicySubject::new("user-456")
            .with_claim("department", serde_json::json!("engineering"));

        assert_eq!(
            subject.claims.get("department"),
            Some(&serde_json::json!("engineering"))
        );
    }

    #[test]
    fn test_policy_env_creation() {
        let env = PolicyEnv::new(1704067200)
            .with_var("region", serde_json::json!("us-west-2"));

        assert_eq!(env.now, 1704067200);
        assert_eq!(
            env.vars.get("region"),
            Some(&serde_json::json!("us-west-2"))
        );
    }

    #[test]
    fn test_policy_context_creation() {
        let subject = PolicySubject::new("user-789").with_roles(["viewer"]);
        let resource = Resource::instance("document", "doc-123");
        let env = PolicyEnv::new(1704067200);

        let ctx = PolicyContext::new(subject, resource, "resource.read", env)
            .with_input(serde_json::json!({"page": 1}));

        assert_eq!(ctx.subject.id, "user-789");
        assert_eq!(ctx.resource.resource_type, "document");
        assert_eq!(ctx.op.as_str(), "resource.read");
        assert!(ctx.input.is_some());
    }
}
