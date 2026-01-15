//! Access configuration types for Row-Level Security.
//!
//! These types represent the persistent access profile configuration
//! that drives capability-bound RLS at compile time.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::filter::FilterOp;
use super::grant::{DataAction, RowPredicate};

// ============================================================================
// Row Scope Types
// ============================================================================

/// Row scope type for RLS.
///
/// CRITICAL: `Deny` is explicit — absence of config does NOT imply permission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RowScopeType {
    /// owner_id = subject.id (kernel default)
    Owner,
    /// TRUE (all rows) — explicit widening
    All,
    /// Custom predicate DSL
    Predicate { dsl: FilterOp },
    /// No access — explicit denial
    Deny,
}

impl RowScopeType {
    /// Convert to RowPredicate for capability embedding.
    pub fn to_predicate(&self) -> RowPredicate {
        match self {
            Self::Owner => RowPredicate::ownership("owner_id"),
            Self::All => RowPredicate::Always,
            Self::Predicate { dsl } => RowPredicate::Sql { filter: dsl.clone() },
            Self::Deny => RowPredicate::Never,
        }
    }

    /// Parse from database string.
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "owner" => Self::Owner,
            "all" => Self::All,
            "deny" => Self::Deny,
            _ => Self::Deny, // Safe default
        }
    }
}

impl Default for RowScopeType {
    /// Kernel default: owner-only access.
    fn default() -> Self {
        Self::Owner
    }
}

// ============================================================================
// Source Tracking (for debugging)
// ============================================================================

/// Source of resolved scope (for debugging and auditing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeSource {
    /// Explicit user-level scope
    User,
    /// Group-level scope
    Group,
    /// Role-level scope
    Role,
    /// Model default scope
    ModelDefault,
    /// Global kernel default (owner_id == subject.id)
    GlobalDefault,
    /// Explicit denial (no match)
    Deny,
}

impl std::fmt::Display for ScopeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Group => write!(f, "group"),
            Self::Role => write!(f, "role"),
            Self::ModelDefault => write!(f, "model_default"),
            Self::GlobalDefault => write!(f, "global_default"),
            Self::Deny => write!(f, "deny"),
        }
    }
}

// ============================================================================
// Principal Type
// ============================================================================

/// Type of principal for access profiles.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrincipalType {
    User,
    Group,
    Role,
}

impl PrincipalType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Group => "group",
            Self::Role => "role",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "group" => Some(Self::Group),
            "role" => Some(Self::Role),
            _ => None,
        }
    }
}

// ============================================================================
// Access Profile
// ============================================================================

/// Access profile for a model.
///
/// Maps a principal (user/group/role) to permitted actions and row scopes.
#[derive(Debug, Clone)]
pub struct AccessProfile {
    /// Unique ID.
    pub id: String,
    /// Target model.
    pub model_id: String,
    /// Principal type (user/group/role).
    pub principal_type: PrincipalType,
    /// Principal identifier.
    pub principal_id: String,
    /// Whether query is allowed.
    pub allow_query: bool,
    /// Whether insert is allowed.
    pub allow_insert: bool,
    /// Whether update is allowed.
    pub allow_update: bool,
    /// Whether delete is allowed.
    pub allow_delete: bool,
    /// Priority (higher wins).
    pub priority: u32,
    /// Per-opcode row scopes.
    pub row_scopes: HashMap<DataAction, RowScopeType>,
}

impl AccessProfile {
    /// Check if an action is allowed.
    pub fn allows(&self, action: DataAction) -> bool {
        match action {
            DataAction::Query | DataAction::Count => self.allow_query,
            DataAction::Insert => self.allow_insert,
            DataAction::Update => self.allow_update,
            DataAction::Delete => self.allow_delete,
        }
    }

    /// Get row scope for an action, falling back to Owner.
    pub fn row_scope(&self, action: DataAction) -> RowScopeType {
        // Count shares query scope
        let effective = match action {
            DataAction::Count => DataAction::Query,
            other => other,
        };
        self.row_scopes
            .get(&effective)
            .cloned()
            .unwrap_or(RowScopeType::Owner)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_scope_type_to_predicate() {
        assert!(matches!(
            RowScopeType::Owner.to_predicate(),
            RowPredicate::Sql { .. }
        ));
        assert_eq!(RowScopeType::All.to_predicate(), RowPredicate::Always);
        assert_eq!(RowScopeType::Deny.to_predicate(), RowPredicate::Never);
    }

    #[test]
    fn test_row_scope_type_default() {
        assert_eq!(RowScopeType::default(), RowScopeType::Owner);
    }

    #[test]
    fn test_row_scope_from_db_str() {
        assert_eq!(RowScopeType::from_db_str("owner"), RowScopeType::Owner);
        assert_eq!(RowScopeType::from_db_str("all"), RowScopeType::All);
        assert_eq!(RowScopeType::from_db_str("deny"), RowScopeType::Deny);
        assert_eq!(RowScopeType::from_db_str("unknown"), RowScopeType::Deny);
    }

    #[test]
    fn test_count_uses_query_scope() {
        let mut scopes = HashMap::new();
        scopes.insert(DataAction::Query, RowScopeType::All);
        scopes.insert(DataAction::Update, RowScopeType::Owner);

        let profile = AccessProfile {
            id: "test".into(),
            model_id: "items".into(),
            principal_type: PrincipalType::Role,
            principal_id: "public".into(),
            allow_query: true,
            allow_insert: false,
            allow_update: false,
            allow_delete: false,
            priority: 0,
            row_scopes: scopes,
        };

        // Count should use query scope
        assert_eq!(profile.row_scope(DataAction::Count), RowScopeType::All);
        assert_eq!(profile.row_scope(DataAction::Query), RowScopeType::All);
        assert_eq!(profile.row_scope(DataAction::Update), RowScopeType::Owner);
    }
}
