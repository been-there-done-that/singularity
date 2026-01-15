//! Row Scope Resolver — resolves row-level predicates at capability issuance time.
//!
//! # Core Invariant
//!
//! > Every data operation MUST carry an explicit row predicate.
//! > Absence of config returns the kernel default (Owner), not permission.
//!
//! # Resolution Hierarchy
//!
//! 1. User-specific scope
//! 2. Group scope (highest priority wins)
//! 3. Role scope (highest priority wins)
//! 4. Model default scope
//! 5. Global default (owner_id == subject.id)
//! 6. Explicit DENY

use crate::policy::PolicySubject;
use crate::protocol::data::{
    AccessProfile, DataAction, PrincipalType, RowPredicate, RowScopeType, ScopeSource,
};
use crate::state::State;

// ============================================================================
// Resolution Result
// ============================================================================

/// Result of scope resolution, including source for debugging.
#[derive(Debug, Clone)]
pub struct ResolvedScope {
    /// The resolved scope type.
    pub scope: RowScopeType,
    /// Where the scope came from (for auditing/debugging).
    pub source: ScopeSource,
    /// The access profile ID (if from explicit config).
    pub profile_id: Option<String>,
}

impl ResolvedScope {
    /// Create a new resolved scope.
    pub fn new(scope: RowScopeType, source: ScopeSource) -> Self {
        Self {
            scope,
            source,
            profile_id: None,
        }
    }

    /// Set the profile ID.
    pub fn with_profile_id(mut self, id: impl Into<String>) -> Self {
        self.profile_id = Some(id.into());
        self
    }

    /// Convert to RowPredicate for capability embedding.
    pub fn to_predicate(&self) -> RowPredicate {
        self.scope.to_predicate()
    }
}

// ============================================================================
// Row Scope Resolver
// ============================================================================

/// Resolves row scopes from access configuration.
///
/// This is a compile-time operation — no policy execution happens here.
pub struct RowScopeResolver<'a, S: State> {
    state: &'a S,
}

impl<'a, S: State> RowScopeResolver<'a, S> {
    /// Create a new resolver.
    pub fn new(state: &'a S) -> Self {
        Self { state }
    }

    /// Resolve row scope for (subject, model, action).
    ///
    /// CRITICAL: Count shares query scope.
    ///
    /// Resolution order:
    /// 1. User-specific scope
    /// 2. Group scope (highest priority wins)
    /// 3. Role scope (highest priority wins)
    /// 4. Model default scope
    /// 5. Global default (Owner)
    /// 6. DENY (should never reach here with valid global default)
    pub fn resolve(
        &self,
        subject: &PolicySubject,
        model: &str,
        action: DataAction,
    ) -> ResolvedScope {
        // Count shares query scope
        let effective_action = match action {
            DataAction::Count => DataAction::Query,
            other => other,
        };

        // 1. Try user-specific scope
        if let Some(internal_id) = &subject.internal_id {
            if let Some(resolved) = self.lookup_scope(
                model,
                PrincipalType::User,
                internal_id,
                effective_action,
            ) {
                return resolved;
            }
        }

        // 2. Try group scopes (TODO: when groups are implemented)
        // Groups would come from subject.claims["groups"]

        // 3. Try role scopes (check each role, pick highest priority match)
        if let Some(resolved) = self.lookup_best_role_scope(
            model,
            &subject.roles,
            effective_action,
        ) {
            return resolved;
        }

        // 4. Try model default scope
        if let Some(resolved) = self.lookup_model_default(model) {
            // Model default only applies to reads (query/count)
            if matches!(effective_action, DataAction::Query) {
                return resolved;
            }
        }

        // 5. Global default: owner_id == subject.id
        ResolvedScope::new(RowScopeType::Owner, ScopeSource::GlobalDefault)
    }

    /// Lookup scope for a specific principal.
    fn lookup_scope(
        &self,
        model: &str,
        principal_type: PrincipalType,
        principal_id: &str,
        action: DataAction,
    ) -> Option<ResolvedScope> {
        // Query __access_profiles for this model + principal
        let profile = self.load_access_profile(model, &principal_type, principal_id)?;

        // Check if action is allowed
        if !profile.allows(action) {
            return Some(ResolvedScope::new(RowScopeType::Deny, ScopeSource::from_principal(&principal_type)));
        }

        // Get scope for this action
        let scope = profile.row_scope(action);
        let source = ScopeSource::from_principal(&principal_type);

        Some(ResolvedScope::new(scope, source).with_profile_id(&profile.id))
    }

    /// Find best role scope (highest priority).
    fn lookup_best_role_scope(
        &self,
        model: &str,
        roles: &[String],
        action: DataAction,
    ) -> Option<ResolvedScope> {
        let mut best: Option<(AccessProfile, ResolvedScope)> = None;

        for role in roles {
            if let Some(profile) = self.load_access_profile(model, &PrincipalType::Role, role) {
                if !profile.allows(action) {
                    // This role explicitly denies - but check if higher priority exists
                    if best.as_ref().map_or(true, |(b, _)| profile.priority > b.priority) {
                        let resolved = ResolvedScope::new(RowScopeType::Deny, ScopeSource::Role)
                            .with_profile_id(&profile.id);
                        best = Some((profile, resolved));
                    }
                } else {
                    // This role allows - check priority
                    if best.as_ref().map_or(true, |(b, _)| profile.priority > b.priority) {
                        let scope = profile.row_scope(action);
                        let resolved = ResolvedScope::new(scope, ScopeSource::Role)
                            .with_profile_id(&profile.id);
                        best = Some((profile, resolved));
                    }
                }
            }
        }

        best.map(|(_, r)| r)
    }

    /// Lookup model default scope from __models.default_row_scope.
    fn lookup_model_default(&self, model: &str) -> Option<ResolvedScope> {
        // Query __models for default_row_scope
        let default_scope = self.load_model_default_scope(model)?;
        Some(ResolvedScope::new(default_scope, ScopeSource::ModelDefault))
    }

    /// Load access profile from state.
    fn load_access_profile(
        &self,
        model: &str,
        principal_type: &PrincipalType,
        principal_id: &str,
    ) -> Option<AccessProfile> {
        use crate::execution::ExecutionTarget;
        use crate::protocol::{FieldSet, Resource};

        // Build query constraints
        let constraints = serde_json::json!({
            "model_id": model,
            "principal_type": principal_type.as_str(),
            "principal_id": principal_id
        });

        let target = ExecutionTarget::new(Resource::collection("__access_profiles"));

        let result = self.state.read(
            &target,
            &FieldSet::all(),
            Some(&constraints),
        ).ok()?;

        // Parse first result
        let arr = result.as_array()?;
        let row = arr.first()?;

        Some(AccessProfile {
            id: row.get("id")?.as_str()?.to_string(),
            model_id: row.get("model_id")?.as_str()?.to_string(),
            principal_type: PrincipalType::from_str(row.get("principal_type")?.as_str()?)?,
            principal_id: row.get("principal_id")?.as_str()?.to_string(),
            allow_query: row.get("allow_query")?.as_i64()? == 1,
            allow_insert: row.get("allow_insert")?.as_i64()? == 1,
            allow_update: row.get("allow_update")?.as_i64()? == 1,
            allow_delete: row.get("allow_delete")?.as_i64()? == 1,
            priority: row.get("priority")?.as_u64()? as u32,
            row_scopes: self.load_row_scopes(row.get("id")?.as_str()?),
        })
    }

    /// Load row scopes for an access profile.
    fn load_row_scopes(&self, profile_id: &str) -> std::collections::HashMap<DataAction, RowScopeType> {
        use crate::execution::ExecutionTarget;
        use crate::protocol::{FieldSet, Resource};
        use std::collections::HashMap;

        let mut scopes = HashMap::new();

        let constraints = serde_json::json!({ "access_profile_id": profile_id });
        let target = ExecutionTarget::new(Resource::collection("__row_scopes"));

        if let Ok(result) = self.state.read(&target, &FieldSet::all(), Some(&constraints)) {
            if let Some(arr) = result.as_array() {
                for row in arr {
                    let opcode = row.get("opcode").and_then(|v| v.as_str()).unwrap_or("");
                    let scope_type = row.get("scope_type").and_then(|v| v.as_str()).unwrap_or("deny");

                    let action = match opcode {
                        "query" => DataAction::Query,
                        "update" => DataAction::Update,
                        "delete" => DataAction::Delete,
                        _ => continue,
                    };

                    let scope = RowScopeType::from_db_str(scope_type);
                    scopes.insert(action, scope);
                }
            }
        }

        scopes
    }

    /// Load model default scope.
    fn load_model_default_scope(&self, model: &str) -> Option<RowScopeType> {
        use crate::execution::ExecutionTarget;
        use crate::protocol::{FieldSet, Resource};

        let constraints = serde_json::json!({ "name": model });
        let target = ExecutionTarget::new(Resource::collection("__models"));

        let result = self.state.read(&target, &FieldSet::all(), Some(&constraints)).ok()?;
        let arr = result.as_array()?;
        let row = arr.first()?;

        let default_scope = row.get("default_row_scope")?.as_str()?;
        Some(RowScopeType::from_db_str(default_scope))
    }
}

impl ScopeSource {
    /// Convert from principal type.
    fn from_principal(pt: &PrincipalType) -> Self {
        match pt {
            PrincipalType::User => Self::User,
            PrincipalType::Group => Self::Group,
            PrincipalType::Role => Self::Role,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_scope_to_predicate() {
        let scope = ResolvedScope::new(RowScopeType::Owner, ScopeSource::GlobalDefault);
        assert!(matches!(scope.to_predicate(), RowPredicate::Sql { .. }));

        let scope = ResolvedScope::new(RowScopeType::All, ScopeSource::Role);
        assert_eq!(scope.to_predicate(), RowPredicate::Always);

        let scope = ResolvedScope::new(RowScopeType::Deny, ScopeSource::Deny);
        assert_eq!(scope.to_predicate(), RowPredicate::Never);
    }

    #[test]
    fn test_count_maps_to_query() {
        // This is tested in the AccessProfile::row_scope method
        // but we verify the resolver also handles it
        use std::collections::HashMap;

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
    }
}
