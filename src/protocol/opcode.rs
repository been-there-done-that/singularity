//! Namespaced operation codes.
//!
//! Operations use namespaced strings for extensibility:
//! - `"resource.create"`, `"resource.read"`, `"resource.update"`, `"resource.delete"`
//! - `"user.profile.read"`, `"order.submit"`, `"admin.audit.view"`

use serde::{Deserialize, Serialize};

/// Namespaced operation code (e.g., "resource.create", "user.profile.read").
///
/// Opcodes are structured as `namespace.action` or `namespace.subnamespace.action`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Opcode(String);

impl Opcode {
    /// Create a new opcode from a string.
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    /// Get the opcode as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the namespace portion of the opcode.
    ///
    /// Returns `Some("resource")` for `"resource.create"`,
    /// `Some("user.profile")` for `"user.profile.read"`,
    /// or `None` if there's no dot separator.
    pub fn namespace(&self) -> Option<&str> {
        self.0.rfind('.').map(|idx| &self.0[..idx])
    }

    /// Get the action portion of the opcode.
    ///
    /// Returns `"create"` for `"resource.create"`,
    /// `"read"` for `"user.profile.read"`,
    /// or the full opcode if there's no dot separator.
    pub fn action(&self) -> &str {
        match self.0.rfind('.') {
            Some(idx) => &self.0[idx + 1..],
            None => &self.0,
        }
    }

    /// Check if this opcode follows the recommended format.
    ///
    /// Recommended format: lowercase alphanumeric with dots as separators.
    /// Example: `"resource.create"`, `"user.profile.read"`
    ///
    /// This is NOT a security check - invalid formats still work.
    /// Use this for hygiene/linting purposes.
    pub fn is_well_formed(&self) -> bool {
        if self.0.is_empty() {
            return false;
        }
        // Must not start or end with dot
        if self.0.starts_with('.') || self.0.ends_with('.') {
            return false;
        }
        // Must not have consecutive dots
        if self.0.contains("..") {
            return false;
        }
        // All characters must be lowercase alphanumeric, underscore, or dot
        self.0.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_')
    }

    /// Create a validated opcode, returning `None` if malformed.
    ///
    /// Use this when you want to enforce well-formed opcodes at construction.
    pub fn try_new(code: impl Into<String>) -> Option<Self> {
        let opcode = Self::new(code);
        if opcode.is_well_formed() {
            Some(opcode)
        } else {
            None
        }
    }
}

impl std::fmt::Display for Opcode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Opcode {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Opcode {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Reserved: Create a new Model.
pub const SCHEMA_CREATE_MODEL: &str = "schema.create_model";
/// Reserved: Add a Field to a Model.
pub const SCHEMA_ADD_FIELD: &str = "schema.add_field";
/// Reserved: Remove a Field from a Model.
pub const SCHEMA_DROP_FIELD: &str = "schema.drop_field";
/// Reserved: Rename a Model.
pub const SCHEMA_RENAME_MODEL: &str = "schema.rename_model";
/// Reserved: Rename a Field.
pub const SCHEMA_RENAME_FIELD: &str = "schema.rename_field";
/// Reserved: List Models.
pub const SCHEMA_LIST_MODELS: &str = "schema.list_models";
/// Reserved: Create an Index on a Model.
pub const SCHEMA_CREATE_INDEX: &str = "schema.create_index";
/// Reserved: Remove an Index from a Model.
pub const SCHEMA_DROP_INDEX: &str = "schema.drop_index";

// Object Plane Opcodes
pub const OBJECT_READ: &str = "object.read";
pub const OBJECT_WRITE: &str = "object.write";
pub const OBJECT_DELETE: &str = "object.delete";
pub const OBJECT_LIST: &str = "object.list";
pub const OBJECT_PRESIGN: &str = "object.presign";

pub const RESOURCE_COUNT: &str = "resource.count";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_creation() {
        let op = Opcode::new("resource.create");
        assert_eq!(op.as_str(), "resource.create");
    }

    #[test]
    fn test_opcode_namespace_simple() {
        let op = Opcode::new("resource.create");
        assert_eq!(op.namespace(), Some("resource"));
        assert_eq!(op.action(), "create");
    }

    #[test]
    fn test_opcode_namespace_nested() {
        let op = Opcode::new("user.profile.read");
        assert_eq!(op.namespace(), Some("user.profile"));
        assert_eq!(op.action(), "read");
    }

    #[test]
    fn test_opcode_no_namespace() {
        let op = Opcode::new("action");
        assert_eq!(op.namespace(), None);
        assert_eq!(op.action(), "action");
    }

    #[test]
    fn test_opcode_from_str() {
        let op: Opcode = "resource.delete".into();
        assert_eq!(op.as_str(), "resource.delete");
    }

    #[test]
    fn test_opcode_display() {
        let op = Opcode::new("order.submit");
        assert_eq!(format!("{}", op), "order.submit");
    }

    #[test]
    fn test_opcode_serialization_roundtrip() {
        let op = Opcode::new("admin.audit.view");
        let json = serde_json::to_string(&op).unwrap();
        let deserialized: Opcode = serde_json::from_str(&json).unwrap();
        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_opcode_well_formed_valid() {
        assert!(Opcode::new("resource.create").is_well_formed());
        assert!(Opcode::new("user.profile.read").is_well_formed());
        assert!(Opcode::new("admin_audit.view").is_well_formed());
        assert!(Opcode::new("action").is_well_formed());
        assert!(Opcode::new("v2.resource.create").is_well_formed());
    }

    #[test]
    fn test_opcode_well_formed_invalid() {
        assert!(!Opcode::new("").is_well_formed());           // empty
        assert!(!Opcode::new(".action").is_well_formed());    // starts with dot
        assert!(!Opcode::new("action.").is_well_formed());    // ends with dot
        assert!(!Opcode::new("a..b").is_well_formed());       // consecutive dots
        assert!(!Opcode::new("Resource.Create").is_well_formed()); // uppercase
        assert!(!Opcode::new("resource create").is_well_formed()); // space
    }

    #[test]
    fn test_opcode_try_new() {
        assert!(Opcode::try_new("resource.create").is_some());
        assert!(Opcode::try_new("INVALID").is_none());
        assert!(Opcode::try_new("").is_none());
    }
}
