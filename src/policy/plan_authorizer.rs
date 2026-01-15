//! Plan Authorization — transforms LogicalPlan + Subject into PlanGrant.
//!
//! # Contract
//!
//! ```text
//! fn authorize(subject, plan, ctx) -> Result<PlanGrant, PolicyError>
//! ```
//!
//! # What This Module Decides
//!
//! Given a `LogicalPlan`, policy must decide exactly five things:
//!
//! 1. **Action Authorization** - allow/deny the operation
//! 2. **Row Predicate** - Always, Never, Sql(filter), or Dynamic(policy_id, fields)
//! 3. **Field Mask** - select_allow, write_allow
//! 4. **Join Grants** - per-join authorization with own row predicates
//! 5. **Bulk Constraints** - max_rows, requires_where, returning_allow
//!
//! # What This Module Does NOT Do
//!
//! - Inspect SQLite
//! - Inspect actual row values
//! - Iterate rows
//! - Compile SQL
//! - Try to "optimize"
//!
//! That all belongs to the Executor (Phase D).

use crate::planner::{LogicalPlan, JoinPlan, MutationPlan};
use crate::protocol::data::{
    BulkGrant, DataAction, FieldMask, FilterOp, FilterValue, JoinGrant, PlanGrant, RowPredicate,
};

use super::context::PolicySubject;
use super::engine::PolicyEngine;

// ============================================================================
// Authorization Error Types
// ============================================================================

/// Errors during plan authorization.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthorizationError {
    /// The action is not permitted for this subject.
    ActionDenied {
        action: DataAction,
        reason: String,
    },

    /// A requested field is not allowed.
    FieldDenied {
        field: String,
        reason: String,
    },

    /// A requested join is not allowed.
    JoinDenied {
        relation: String,
        reason: String,
    },

    /// Bulk operation constraints violated.
    BulkDenied {
        reason: String,
    },

    /// The plan itself is invalid (400, not 403).
    InvalidPlan {
        reason: String,
    },
}

impl std::fmt::Display for AuthorizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActionDenied { action, reason } => {
                write!(f, "action {:?} denied: {}", action, reason)
            }
            Self::FieldDenied { field, reason } => {
                write!(f, "field '{}' denied: {}", field, reason)
            }
            Self::JoinDenied { relation, reason } => {
                write!(f, "join on '{}' denied: {}", relation, reason)
            }
            Self::BulkDenied { reason } => {
                write!(f, "bulk operation denied: {}", reason)
            }
            Self::InvalidPlan { reason } => {
                write!(f, "invalid plan: {}", reason)
            }
        }
    }
}

impl std::error::Error for AuthorizationError {}

// ============================================================================
// Authorization Context
// ============================================================================

/// Context for plan authorization decisions.
///
/// This is separate from `PolicyContext` because it operates at
/// the LogicalPlan level, not the raw request level.
#[derive(Debug, Clone)]
pub struct PlanAuthContext {
    /// The authenticated subject.
    pub subject: PolicySubject,

    /// Current timestamp (for time-based policies).
    pub now: u64,

    /// Policy configuration for this model.
    pub policy_config: ModelPolicyConfig,
}

/// Policy configuration for a specific model.
///
/// This would typically come from model metadata or a policy store.
#[derive(Debug, Clone, Default)]
pub struct ModelPolicyConfig {
    /// Whether this model is public (no auth required).
    pub is_public: bool,

    /// The field used for ownership checks (e.g., "owner_id").
    pub owner_field: Option<String>,

    /// Roles that have admin-level access (bypass row predicates).
    pub admin_roles: Vec<String>,

    /// Maximum rows for bulk operations.
    pub max_bulk_rows: Option<u32>,

    /// Fields that are never readable (e.g., "password_hash").
    pub forbidden_read_fields: Vec<String>,

    /// Fields that are never writable (e.g., "id", "created_at").
    pub forbidden_write_fields: Vec<String>,

    /// Relations that are allowed to be joined.
    pub allowed_relations: Vec<String>,
}

// ============================================================================
// Plan Authorizer
// ============================================================================

/// Authorizes a LogicalPlan and produces a PlanGrant.
///
/// This is the core Phase C function.
pub struct PlanAuthorizer {
    _engine: PolicyEngine,
}

impl Default for PlanAuthorizer {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanAuthorizer {
    /// Create a new plan authorizer.
    pub fn new() -> Self {
        Self {
            _engine: PolicyEngine::new(),
        }
    }

    /// Authorize a LogicalPlan and produce a PlanGrant.
    ///
    /// # Arguments
    ///
    /// * `plan` - The LogicalPlan from Phase B
    /// * `ctx` - Authorization context (subject, config)
    ///
    /// # Returns
    ///
    /// * `Ok(PlanGrant)` - Authorization succeeded, execution may proceed
    /// * `Err(AuthorizationError)` - Authorization failed (403)
    pub fn authorize(
        &self,
        plan: &LogicalPlan,
        ctx: &PlanAuthContext,
    ) -> Result<PlanGrant, AuthorizationError> {
        // Step 1: Action authorization
        self.authorize_action(plan.action, ctx)?;

        // Step 2: Compute row predicate
        let row_predicate = self.compute_row_predicate(plan, ctx);

        // Step 3: Compute field masks
        let field_mask = self.compute_field_mask(plan, ctx)?;

        // Step 4: Authorize and grant joins
        let joins = self.authorize_joins(&plan.joins, ctx)?;

        // Step 5: Compute bulk constraints
        let bulk = self.compute_bulk_grant(plan, ctx)?;

        Ok(PlanGrant {
            model: plan.base_model.name.clone(),
            action: plan.action,
            row_predicate,
            field_mask,
            joins,
            bulk,
        })
    }

    /// Step 1: Authorize the action itself.
    fn authorize_action(
        &self,
        action: DataAction,
        ctx: &PlanAuthContext,
    ) -> Result<(), AuthorizationError> {
        // Public models allow all reads
        if ctx.policy_config.is_public && matches!(action, DataAction::Query | DataAction::Count) {
            return Ok(());
        }

        // Admins can do anything
        if self.is_admin(ctx) {
            return Ok(());
        }

        // For now, authenticated users can perform basic operations
        // In production, this would consult the policy engine
        if !ctx.subject.id.is_empty() {
            return Ok(());
        }

        Err(AuthorizationError::ActionDenied {
            action,
            reason: "subject not authenticated".into(),
        })
    }

    /// Step 2: Compute the row-level predicate.
    fn compute_row_predicate(&self, plan: &LogicalPlan, ctx: &PlanAuthContext) -> RowPredicate {
        // Public models: always allow
        if ctx.policy_config.is_public
            && matches!(plan.action, DataAction::Query | DataAction::Count)
        {
            return RowPredicate::Always;
        }

        // Admins: always allow
        if self.is_admin(ctx) {
            return RowPredicate::Always;
        }

        // Ownership-based access
        if let Some(ref owner_field) = ctx.policy_config.owner_field {
            // Generate: WHERE owner_field = $subject
            return RowPredicate::Sql {
                filter: FilterOp::Eq(owner_field.clone(), FilterValue::Subject),
            };
        }

        // Default: deny all (safest default)
        RowPredicate::Never
    }

    /// Step 3: Compute field-level access masks.
    fn compute_field_mask(
        &self,
        plan: &LogicalPlan,
        ctx: &PlanAuthContext,
    ) -> Result<FieldMask, AuthorizationError> {
        // Extract requested fields
        let requested_select: Vec<String> = plan
            .selection
            .base_fields
            .iter()
            .map(|f| f.name.clone())
            .collect();

        // Check for forbidden read fields
        for field in &requested_select {
            if ctx.policy_config.forbidden_read_fields.contains(field) {
                return Err(AuthorizationError::FieldDenied {
                    field: field.clone(),
                    reason: "field is not readable".into(),
                });
            }
        }

        // For mutations, check write fields
        let mut requested_write = Vec::new();
        if let Some(ref mutation) = plan.mutation {
            match mutation {
                MutationPlan::Insert { .. } => {
                    // All fields in insert rows are being written
                    // We'd need to extract field names from the rows
                    // For now, allow all non-forbidden fields
                }
                MutationPlan::Update { set, .. } => {
                    for (field, _) in set {
                        if ctx.policy_config.forbidden_write_fields.contains(&field.name) {
                            return Err(AuthorizationError::FieldDenied {
                                field: field.name.clone(),
                                reason: "field is not writable".into(),
                            });
                        }
                        requested_write.push(field.name.clone());
                    }
                }
                MutationPlan::Delete { .. } => {
                    // Delete doesn't write fields
                }
            }
        }

        Ok(FieldMask {
            select_allow: if requested_select.is_empty() || requested_select.contains(&"*".to_string()) {
                vec!["*".into()]
            } else {
                requested_select
            },
            write_allow: if requested_write.is_empty() {
                // For inserts, allow all non-forbidden
                vec!["*".into()]
            } else {
                requested_write
            },
        })
    }

    /// Step 4: Authorize and grant joins.
    fn authorize_joins(
        &self,
        joins: &[JoinPlan],
        ctx: &PlanAuthContext,
    ) -> Result<Vec<JoinGrant>, AuthorizationError> {
        let mut grants = Vec::new();

        for join in joins {
            // Check if relation is allowed
            if !ctx.policy_config.allowed_relations.is_empty()
                && !ctx.policy_config.allowed_relations.contains(&join.relation.name)
            {
                return Err(AuthorizationError::JoinDenied {
                    relation: join.relation.name.clone(),
                    reason: "relation not in allowed list".into(),
                });
            }

            // Compute row predicate for the joined model
            // For now, use the same logic as the base model
            let row_predicate = if self.is_admin(ctx) {
                RowPredicate::Always
            } else if let Some(ref owner_field) = ctx.policy_config.owner_field {
                // Apply ownership on joined table too
                RowPredicate::Sql {
                    filter: FilterOp::Eq(owner_field.clone(), FilterValue::Subject),
                }
            } else {
                RowPredicate::Always // Default to allow for joins
            };

            grants.push(JoinGrant {
                relation: join.relation.name.clone(),
                allow_types: vec![join.join_type.clone()],
                select_allow: join
                    .selected_fields
                    .iter()
                    .map(|f| f.name.clone())
                    .collect(),
                row_predicate,
            });
        }

        Ok(grants)
    }

    /// Step 5: Compute bulk operation constraints.
    fn compute_bulk_grant(
        &self,
        plan: &LogicalPlan,
        ctx: &PlanAuthContext,
    ) -> Result<BulkGrant, AuthorizationError> {
        let is_bulk = matches!(
            plan.action,
            DataAction::Update | DataAction::Delete | DataAction::Insert
        );

        if !is_bulk {
            // Query/Count don't need bulk constraints
            return Ok(BulkGrant {
                allow: true,
                requires_where: false,
                max_rows: None,
                returning_allow: vec!["*".into()],
            });
        }

        // Admins get relaxed constraints
        let max_rows = if self.is_admin(ctx) {
            None // No limit for admins
        } else {
            ctx.policy_config.max_bulk_rows.or(Some(1000))
        };

        // Extract returning fields from mutation
        let returning_allow = match &plan.mutation {
            Some(MutationPlan::Insert { returning, .. })
            | Some(MutationPlan::Update { returning, .. })
            | Some(MutationPlan::Delete { returning, .. }) => {
                returning.iter().map(|f| f.name.clone()).collect()
            }
            None => vec![],
        };

        Ok(BulkGrant {
            allow: true,
            requires_where: !self.is_admin(ctx), // Non-admins require WHERE
            max_rows,
            returning_allow,
        })
    }

    /// Check if the subject has admin role.
    fn is_admin(&self, ctx: &PlanAuthContext) -> bool {
        ctx.subject
            .roles
            .iter()
            .any(|r| ctx.policy_config.admin_roles.contains(r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::{
        LogicalPlan, SelectionPlan, FieldRef, FieldType, ModelRef,
    };

    fn test_model() -> ModelRef {
        ModelRef::new("items", "model-items")
    }

    fn test_field(name: &str) -> FieldRef {
        FieldRef::new(test_model(), name, FieldType::String)
    }

    fn minimal_query_plan() -> LogicalPlan {
        LogicalPlan {
            action: DataAction::Query,
            base_model: test_model(),
            selection: SelectionPlan::new(vec![test_field("id"), test_field("title")]),
            joins: vec![],
            filter: None,
            mutation: None,
            ordering: vec![],
            pagination: None,
        }
    }

    fn admin_context() -> PlanAuthContext {
        PlanAuthContext {
            subject: PolicySubject::new("admin-user")
                .with_roles(["admin"])
                .with_internal_id("internal-admin"),
            now: 1704067200,
            policy_config: ModelPolicyConfig {
                admin_roles: vec!["admin".into()],
                owner_field: Some("owner_id".into()),
                ..Default::default()
            },
        }
    }

    fn user_context() -> PlanAuthContext {
        PlanAuthContext {
            subject: PolicySubject::new("regular-user")
                .with_roles(["user"])
                .with_internal_id("internal-user-123"),
            now: 1704067200,
            policy_config: ModelPolicyConfig {
                admin_roles: vec!["admin".into()],
                owner_field: Some("owner_id".into()),
                max_bulk_rows: Some(100),
                ..Default::default()
            },
        }
    }

    fn public_context() -> PlanAuthContext {
        PlanAuthContext {
            subject: PolicySubject::new("anonymous"),
            now: 1704067200,
            policy_config: ModelPolicyConfig {
                is_public: true,
                ..Default::default()
            },
        }
    }

    // ==================== Action Authorization ====================

    #[test]
    fn test_admin_can_query() {
        let authorizer = PlanAuthorizer::new();
        let plan = minimal_query_plan();
        let ctx = admin_context();

        let grant = authorizer.authorize(&plan, &ctx).unwrap();
        assert_eq!(grant.action, DataAction::Query);
    }

    #[test]
    fn test_user_can_query() {
        let authorizer = PlanAuthorizer::new();
        let plan = minimal_query_plan();
        let ctx = user_context();

        let grant = authorizer.authorize(&plan, &ctx).unwrap();
        assert_eq!(grant.action, DataAction::Query);
    }

    #[test]
    fn test_public_model_allows_anonymous_read() {
        let authorizer = PlanAuthorizer::new();
        let plan = minimal_query_plan();
        let ctx = public_context();

        let grant = authorizer.authorize(&plan, &ctx).unwrap();
        assert_eq!(grant.row_predicate, RowPredicate::Always);
    }

    // ==================== Row Predicate ====================

    #[test]
    fn test_admin_gets_always_predicate() {
        let authorizer = PlanAuthorizer::new();
        let plan = minimal_query_plan();
        let ctx = admin_context();

        let grant = authorizer.authorize(&plan, &ctx).unwrap();
        assert_eq!(grant.row_predicate, RowPredicate::Always);
    }

    #[test]
    fn test_user_gets_ownership_predicate() {
        let authorizer = PlanAuthorizer::new();
        let plan = minimal_query_plan();
        let ctx = user_context();

        let grant = authorizer.authorize(&plan, &ctx).unwrap();
        
        match grant.row_predicate {
            RowPredicate::Sql { filter: FilterOp::Eq(field, FilterValue::Subject) } => {
                assert_eq!(field, "owner_id");
            }
            other => panic!("expected Sql predicate with owner_id, got {:?}", other),
        }
    }

    // ==================== Field Mask ====================

    #[test]
    fn test_field_mask_includes_requested_fields() {
        let authorizer = PlanAuthorizer::new();
        let plan = minimal_query_plan();
        let ctx = user_context();

        let grant = authorizer.authorize(&plan, &ctx).unwrap();
        assert!(grant.field_mask.select_allow.contains(&"id".to_string()));
        assert!(grant.field_mask.select_allow.contains(&"title".to_string()));
    }

    #[test]
    fn test_forbidden_read_field_denied() {
        let authorizer = PlanAuthorizer::new();
        
        // Request a forbidden field
        let mut plan = minimal_query_plan();
        plan.selection = SelectionPlan::new(vec![
            test_field("id"),
            test_field("password_hash"),
        ]);
        
        let mut ctx = user_context();
        ctx.policy_config.forbidden_read_fields = vec!["password_hash".into()];

        let err = authorizer.authorize(&plan, &ctx).unwrap_err();
        assert!(matches!(err, AuthorizationError::FieldDenied { field, .. } if field == "password_hash"));
    }

    // ==================== Bulk Constraints ====================

    #[test]
    fn test_user_has_max_rows_limit() {
        let authorizer = PlanAuthorizer::new();
        
        let mut plan = minimal_query_plan();
        plan.action = DataAction::Update;
        plan.filter = Some(FilterOp::Eq("status".into(), FilterValue::String("draft".into())));
        plan.mutation = Some(MutationPlan::Update {
            set: vec![(test_field("status"), serde_json::json!("published"))],
            returning: vec![test_field("id")],
        });
        
        let ctx = user_context();

        let grant = authorizer.authorize(&plan, &ctx).unwrap();
        assert_eq!(grant.bulk.max_rows, Some(100));
        assert!(grant.bulk.requires_where);
    }

    #[test]
    fn test_admin_no_bulk_limit() {
        let authorizer = PlanAuthorizer::new();
        
        let mut plan = minimal_query_plan();
        plan.action = DataAction::Update;
        plan.filter = Some(FilterOp::Eq("status".into(), FilterValue::String("draft".into())));
        plan.mutation = Some(MutationPlan::Update {
            set: vec![(test_field("status"), serde_json::json!("published"))],
            returning: vec![],
        });
        
        let ctx = admin_context();

        let grant = authorizer.authorize(&plan, &ctx).unwrap();
        assert_eq!(grant.bulk.max_rows, None); // No limit for admin
        assert!(!grant.bulk.requires_where);   // Admin doesn't require WHERE
    }

    // ==================== Join Authorization ====================

    #[test]
    fn test_allowed_join_succeeds() {
        use crate::planner::{JoinPlan, RelationRef};
        use crate::protocol::data::JoinType;

        let authorizer = PlanAuthorizer::new();
        
        let users_model = ModelRef::new("users", "model-users");
        let relation = RelationRef::new(
            "items.owner",
            test_model(),
            test_field("owner_id"),
            users_model.clone(),
            FieldRef::new(users_model.clone(), "id", FieldType::String),
        );
        
        let mut plan = minimal_query_plan();
        plan.joins = vec![JoinPlan::new(
            relation,
            "owner",
            JoinType::Left,
            vec![FieldRef::new(users_model, "email", FieldType::String)],
        )];
        
        let mut ctx = user_context();
        ctx.policy_config.allowed_relations = vec!["items.owner".into()];

        let grant = authorizer.authorize(&plan, &ctx).unwrap();
        assert_eq!(grant.joins.len(), 1);
        assert_eq!(grant.joins[0].relation, "items.owner");
    }

    #[test]
    fn test_forbidden_join_denied() {
        use crate::planner::{JoinPlan, RelationRef};
        use crate::protocol::data::JoinType;

        let authorizer = PlanAuthorizer::new();
        
        let users_model = ModelRef::new("users", "model-users");
        let relation = RelationRef::new(
            "items.owner",
            test_model(),
            test_field("owner_id"),
            users_model.clone(),
            FieldRef::new(users_model.clone(), "id", FieldType::String),
        );
        
        let mut plan = minimal_query_plan();
        plan.joins = vec![JoinPlan::new(
            relation,
            "owner",
            JoinType::Left,
            vec![FieldRef::new(users_model, "email", FieldType::String)],
        )];
        
        let mut ctx = user_context();
        ctx.policy_config.allowed_relations = vec!["items.category".into()]; // Different relation

        let err = authorizer.authorize(&plan, &ctx).unwrap_err();
        assert!(matches!(err, AuthorizationError::JoinDenied { relation, .. } if relation == "items.owner"));
    }
}
