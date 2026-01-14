//! Shared types used across protocol messages.

use serde::{Deserialize, Serialize};

/// Resource identifier specifying type and optional ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    /// The type of resource (e.g., "user", "document", "order").
    pub resource_type: String,
    /// Optional specific resource ID. `None` for collection-level operations.
    pub resource_id: Option<String>,
}

impl Resource {
    /// Create a new resource with type and optional ID.
    pub fn new(resource_type: impl Into<String>, resource_id: Option<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            resource_id,
        }
    }

    /// Create a resource targeting a specific instance.
    pub fn instance(resource_type: impl Into<String>, resource_id: impl Into<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            resource_id: Some(resource_id.into()),
        }
    }

    /// Create a resource targeting a collection (no specific ID).
    pub fn collection(resource_type: impl Into<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            resource_id: None,
        }
    }
}

/// A set of field names for field-level access control.
///
/// # Semantics
///
/// - **Empty vector (`fields: []`)**: Explicitly grants access to NO fields.
///   Use this when you want to authorize an operation but restrict all field access.
/// - **Missing/None in parent struct**: The operation doesn't specify field-level
///   restrictions; policy or defaults apply.
/// - **Wildcard (`fields: ["*"]`)**: Access to ALL fields. Use [`FieldSet::all()`].
///
/// When checking access, use [`contains()`](Self::contains) which handles the wildcard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSet {
    /// The fields included in this set. Empty means no fields; `["*"]` means all.
    pub fields: Vec<String>,
}

impl FieldSet {
    /// Create a new field set from a list of field names.
    pub fn new(fields: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            fields: fields.into_iter().map(Into::into).collect(),
        }
    }

    /// Create an empty field set.
    pub fn empty() -> Self {
        Self { fields: Vec::new() }
    }

    /// Create a field set representing all fields.
    pub fn all() -> Self {
        Self {
            fields: vec!["*".to_string()],
        }
    }

    /// Check if this field set contains a specific field.
    pub fn contains(&self, field: &str) -> bool {
        self.fields.iter().any(|f| f == "*" || f == field)
    }

    /// Check if this field set represents all fields.
    pub fn is_all(&self) -> bool {
        self.fields.iter().any(|f| f == "*")
    }
}

/// Context binding for capability tokens.
///
/// Used to bind a capability to specific client context, preventing replay attacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityBinding {
    /// Client-provided nonce for request binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_nonce: Option<String>,
    /// IP address hint for context binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_hint: Option<String>,
}

impl CapabilityBinding {
    /// Create a new capability binding.
    pub fn new(client_nonce: Option<String>, ip_hint: Option<String>) -> Self {
        Self {
            client_nonce,
            ip_hint,
        }
    }

    /// Create a binding with only a client nonce.
    pub fn with_nonce(nonce: impl Into<String>) -> Self {
        Self {
            client_nonce: Some(nonce.into()),
            ip_hint: None,
        }
    }

    /// Create a binding with only an IP hint.
    pub fn with_ip(ip: impl Into<String>) -> Self {
        Self {
            client_nonce: None,
            ip_hint: Some(ip.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_instance() {
        let r = Resource::instance("user", "123");
        assert_eq!(r.resource_type, "user");
        assert_eq!(r.resource_id, Some("123".to_string()));
    }

    #[test]
    fn test_resource_collection() {
        let r = Resource::collection("documents");
        assert_eq!(r.resource_type, "documents");
        assert_eq!(r.resource_id, None);
    }

    #[test]
    fn test_field_set_contains() {
        let fs = FieldSet::new(["name", "email", "age"]);
        assert!(fs.contains("name"));
        assert!(fs.contains("email"));
        assert!(!fs.contains("password"));
    }

    #[test]
    fn test_field_set_all() {
        let fs = FieldSet::all();
        assert!(fs.is_all());
        assert!(fs.contains("anything"));
        assert!(fs.contains("literally_any_field"));
    }

    #[test]
    fn test_capability_binding() {
        let binding = CapabilityBinding::new(Some("nonce123".to_string()), Some("192.168.1.1".to_string()));
        assert_eq!(binding.client_nonce, Some("nonce123".to_string()));
        assert_eq!(binding.ip_hint, Some("192.168.1.1".to_string()));
    }

    #[test]
    fn test_resource_serialization_roundtrip() {
        let r = Resource::instance("order", "ord-456");
        let json = serde_json::to_string(&r).unwrap();
        let deserialized: Resource = serde_json::from_str(&json).unwrap();
        assert_eq!(r, deserialized);
    }

    #[test]
    fn test_field_set_serialization_roundtrip() {
        let fs = FieldSet::new(["id", "name", "created_at"]);
        let json = serde_json::to_string(&fs).unwrap();
        let deserialized: FieldSet = serde_json::from_str(&json).unwrap();
        assert_eq!(fs, deserialized);
    }
}
