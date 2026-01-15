//! Access profile input/output types for admin operations.
//!
//! These types define the request/response shapes for access profile CRUD.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::access_config::{PrincipalType, RowScopeType};

// ============================================================================
// Input Types
// ============================================================================

/// Input for creating an access profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessProfileInput {
    /// Target model.
    pub model_id: String,
    /// Principal type (user/group/role).
    pub principal_type: PrincipalType,
    /// Principal identifier.
    pub principal_id: String,
    /// Whether query is allowed.
    #[serde(default)]
    pub allow_query: bool,
    /// Whether insert is allowed.
    #[serde(default)]
    pub allow_insert: bool,
    /// Whether update is allowed.
    #[serde(default)]
    pub allow_update: bool,
    /// Whether delete is allowed.
    #[serde(default)]
    pub allow_delete: bool,
    /// Priority (higher wins).
    #[serde(default)]
    pub priority: u32,
    /// Per-opcode row scopes.
    #[serde(default)]
    pub row_scopes: HashMap<String, RowScopeType>,
}

impl AccessProfileInput {
    /// Validate the input and ensure safe defaults.
    pub fn validate(&self) -> Result<(), AccessProfileError> {
        if self.model_id.is_empty() {
            return Err(AccessProfileError::InvalidInput("model_id is required".into()));
        }
        if self.principal_id.is_empty() {
            return Err(AccessProfileError::InvalidInput("principal_id is required".into()));
        }
        Ok(())
    }

    /// Ensure every allowed action has an explicit scope (defaults to Deny).
    pub fn with_safe_defaults(mut self) -> Self {
        if self.allow_query && !self.row_scopes.contains_key("query") {
            self.row_scopes.insert("query".into(), RowScopeType::Deny);
        }
        if self.allow_update && !self.row_scopes.contains_key("update") {
            self.row_scopes.insert("update".into(), RowScopeType::Deny);
        }
        if self.allow_delete && !self.row_scopes.contains_key("delete") {
            self.row_scopes.insert("delete".into(), RowScopeType::Deny);
        }
        self
    }
}

/// Input for updating an access profile.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccessProfileUpdate {
    /// New query permission (if provided).
    pub allow_query: Option<bool>,
    /// New insert permission (if provided).
    pub allow_insert: Option<bool>,
    /// New update permission (if provided).
    pub allow_update: Option<bool>,
    /// New delete permission (if provided).
    pub allow_delete: Option<bool>,
    /// New priority (if provided).
    pub priority: Option<u32>,
    /// Row scope updates (merged with existing).
    #[serde(default)]
    pub row_scopes: HashMap<String, RowScopeType>,
}

// ============================================================================
// Output Types
// ============================================================================

/// Output for a single access profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessProfileOutput {
    /// Unique ID.
    pub id: String,
    /// Target model.
    pub model_id: String,
    /// Principal type.
    pub principal_type: PrincipalType,
    /// Principal identifier.
    pub principal_id: String,
    /// Query permission.
    pub allow_query: bool,
    /// Insert permission.
    pub allow_insert: bool,
    /// Update permission.
    pub allow_update: bool,
    /// Delete permission.
    pub allow_delete: bool,
    /// Priority.
    pub priority: u32,
    /// Per-opcode row scopes.
    pub row_scopes: HashMap<String, RowScopeType>,
    /// Created timestamp.
    pub created_at: u64,
    /// Updated timestamp.
    pub updated_at: u64,
}

/// Output for listing access profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessProfileListOutput {
    /// List of profiles.
    pub profiles: Vec<AccessProfileOutput>,
    /// Total count.
    pub total: usize,
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors for access profile operations.
#[derive(Debug, Clone, PartialEq)]
pub enum AccessProfileError {
    /// Invalid input data.
    InvalidInput(String),
    /// Profile not found.
    NotFound(String),
    /// Duplicate profile (model + principal already exists).
    Conflict(String),
    /// Internal error.
    Internal(String),
}

impl std::fmt::Display for AccessProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid input: {}", msg),
            Self::NotFound(id) => write!(f, "profile not found: {}", id),
            Self::Conflict(msg) => write!(f, "conflict: {}", msg),
            Self::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

impl std::error::Error for AccessProfileError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_with_safe_defaults() {
        let input = AccessProfileInput {
            model_id: "todo".into(),
            principal_type: PrincipalType::Role,
            principal_id: "public".into(),
            allow_query: true,
            allow_insert: false,
            allow_update: false,
            allow_delete: false,
            priority: 0,
            row_scopes: HashMap::new(),
        };

        let input = input.with_safe_defaults();
        
        // Query is allowed but no scope was provided → should default to Deny
        assert_eq!(input.row_scopes.get("query"), Some(&RowScopeType::Deny));
        // Update is not allowed → no scope inserted
        assert_eq!(input.row_scopes.get("update"), None);
    }

    #[test]
    fn test_input_validation() {
        let input = AccessProfileInput {
            model_id: "".into(),
            principal_type: PrincipalType::Role,
            principal_id: "public".into(),
            allow_query: true,
            allow_insert: false,
            allow_update: false,
            allow_delete: false,
            priority: 0,
            row_scopes: HashMap::new(),
        };

        assert!(matches!(input.validate(), Err(AccessProfileError::InvalidInput(_))));
    }
}
