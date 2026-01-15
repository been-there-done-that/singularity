//! Policy engine - evaluates policies and returns YES/NO.
//!
//! # Invariant
//!
//! > **Policies decide YES/NO only.**
//! >
//! > They do NOT:
//! > - mint authority
//! > - perform execution
//! > - modify state
//! > - widen access
//! > - observe side effects
//!
//! If this invariant holds, policy bugs **cannot become security bugs**.
//!
//! # Minting Flow
//!
//! The engine does NOT mint capabilities. The flow is:
//!
//! ```text
//! PolicyEngine::evaluate(script, ctx) -> bool
//!                  ↓
//!         (caller decides)
//!                  ↓
//!       CapabilitySigner::mint(payload)
//! ```
//!
//! This keeps policy = judgment, capability = authority.

use rhai::{Dynamic, Scope};
use serde::Serialize;

use super::context::PolicyContext;
use super::error::PolicyError;
use super::sandbox::create_sandboxed_engine;

// ============================================================================
// Authorization Explainability Types
// ============================================================================

/// The result of a policy evaluation with full explanation.
///
/// This struct is for **debugging and admin introspection only**.
/// It does NOT change the authorization contract.
#[derive(Debug, Clone, Serialize)]
pub struct AccessDecision {
    /// Final decision: true = allow, false = deny
    pub allowed: bool,
    /// The operation that was evaluated
    pub op: String,
    /// Subject information used in decision
    pub subject: SubjectSnapshot,
    /// Ownership information
    pub ownership: OwnershipCheck,
    /// Policy evaluation details
    pub policy: PolicyEvaluation,
}

/// Immutable snapshot of subject identity for explainability.
#[derive(Debug, Clone, Serialize)]
pub struct SubjectSnapshot {
    /// External subject ID
    pub id: String,
    /// Internal (kernel-managed) ID
    pub internal_id: Option<String>,
    /// Subject's roles
    pub roles: Vec<String>,
}

/// Ownership check result for explainability.
#[derive(Debug, Clone, Serialize)]
pub struct OwnershipCheck {
    /// Column used for ownership (e.g., "owner_id")
    pub column: Option<String>,
    /// Whether subject is resource owner
    pub is_owner: bool,
}

/// Policy evaluation details for explainability.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyEvaluation {
    /// Source of the policy script
    pub script_source: ScriptSource,
    /// The final result of the policy
    pub result: PolicyResult,
}

/// Source of the policy script.
#[derive(Debug, Clone, Serialize)]
pub enum ScriptSource {
    /// Built-in default policy
    Default,
    /// Inline script content
    Inline(String),
}

/// Result of policy evaluation.
#[derive(Debug, Clone, Serialize)]
pub enum PolicyResult {
    /// Policy allows the operation
    Allow,
    /// Policy denies the operation
    Deny,
    /// Policy evaluation failed
    Error(String),
}

// ============================================================================
// Policy Engine
// ============================================================================

/// Policy engine for evaluating policy scripts.
///
/// The engine is sandboxed and does NOT hold a signer.
/// It returns only `bool` - the caller is responsible for minting.
pub struct PolicyEngine {
    engine: rhai::Engine,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEngine {
    /// Create a new policy engine with sandboxed Rhai runtime.
    pub fn new() -> Self {
        Self {
            engine: create_sandboxed_engine(),
        }
    }

    /// Evaluate a policy script and return the decision.
    ///
    /// # Arguments
    ///
    /// * `script` - Rhai policy script (expression returning bool)
    /// * `ctx` - Read-only policy context
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Policy grants the request
    /// * `Ok(false)` - Policy denies the request
    /// * `Err(...)` - Policy evaluation failed
    ///
    /// # Example Policy Script
    ///
    /// ```rhai
    /// let is_owner = subject_id == resource_id;
    /// let is_admin = roles.contains("admin");
    /// (is_owner || is_admin) && op == "resource.read"
    /// ```
    pub fn evaluate(&self, script: &str, ctx: &PolicyContext) -> Result<bool, PolicyError> {
        // Create scope with read-only context variables
        let mut scope = self.build_scope(ctx);

        // Evaluate the policy script
        let result: Dynamic = self
            .engine
            .eval_with_scope(&mut scope, script)
            .map_err(|e| {
                // Classify error as sandbox violation or evaluation failure
                Self::classify_rhai_error(e)
            })?;

        // Policy must return boolean
        if result.is_bool() {
            Ok(result.as_bool().unwrap())
        } else {
            Err(PolicyError::NonBooleanResult(result.type_name().to_string()))
        }
    }

    /// Evaluate a policy script and return an explained decision.
    ///
    /// This is for **debugging and admin introspection only**.
    /// It does NOT change the authorization contract.
    ///
    /// # Arguments
    ///
    /// * `script` - Rhai policy script (expression returning bool), or None for default
    /// * `ctx` - Read-only policy context
    ///
    /// # Returns
    ///
    /// An `AccessDecision` containing the decision and full explanation.
    pub fn evaluate_with_explanation(
        &self,
        script: Option<&str>,
        ctx: &PolicyContext,
    ) -> AccessDecision {
        // 1. Capture subject snapshot
        let subject = SubjectSnapshot {
            id: ctx.subject.id.clone(),
            internal_id: ctx.subject.internal_id.clone(),
            roles: ctx.subject.roles.clone(),
        };

        // 2. Compute ownership check
        let is_owner = match (&ctx.resource_owner, &ctx.subject.internal_id) {
            (Some(res_owner), Some(sub_int)) => res_owner == sub_int,
            _ => false,
        };
        let ownership = OwnershipCheck {
            column: if ctx.resource_owner.is_some() {
                Some("owner_id".to_string())
            } else {
                None
            },
            is_owner,
        };

        // 3. Determine script source and evaluate
        let (script_source, policy_result, allowed) = match script {
            Some(s) => {
                // Inline script evaluation
                match self.evaluate(s, ctx) {
                    Ok(true) => (ScriptSource::Inline(s.to_string()), PolicyResult::Allow, true),
                    Ok(false) => (ScriptSource::Inline(s.to_string()), PolicyResult::Deny, false),
                    Err(e) => (
                        ScriptSource::Inline(s.to_string()),
                        PolicyResult::Error(e.to_string()),
                        false,
                    ),
                }
            }
            None => {
                // Default policy: owner check only
                if is_owner {
                    (ScriptSource::Default, PolicyResult::Allow, true)
                } else {
                    (ScriptSource::Default, PolicyResult::Deny, false)
                }
            }
        };

        let policy = PolicyEvaluation {
            script_source,
            result: policy_result,
        };

        AccessDecision {
            allowed,
            op: ctx.op.as_str().to_string(),
            subject,
            ownership,
            policy,
        }
    }

    /// Classify a Rhai error as sandbox violation or evaluation failure.
    fn classify_rhai_error(e: Box<rhai::EvalAltResult>) -> PolicyError {
        use rhai::EvalAltResult;

        match *e {
            // Disabled keywords/symbols
            EvalAltResult::ErrorParsing(ref kind, _) => {
                let msg = kind.to_string();
                let msg_lower = msg.to_lowercase();
                // Reserved keywords, disabled symbols, banned syntax
                if msg_lower.contains("reserved") 
                    || msg_lower.contains("disabled")
                    || msg_lower.contains("keyword")
                    || msg_lower.contains("forbidden")
                {
                    PolicyError::SandboxViolation(format!("blocked syntax: {}", msg))
                } else {
                    PolicyError::InvalidPolicy(msg)
                }
            }
            // Operation limits
            EvalAltResult::ErrorTooManyOperations(_) => {
                PolicyError::SandboxViolation("operation limit exceeded".to_string())
            }
            // Stack overflow
            EvalAltResult::ErrorStackOverflow(_) => {
                PolicyError::SandboxViolation("stack overflow".to_string())
            }
            // Data size limits
            EvalAltResult::ErrorDataTooLarge(ref msg, _) => {
                PolicyError::SandboxViolation(format!("data size exceeded: {}", msg))
            }
            // General runtime errors
            other => PolicyError::EvaluationFailed(other.to_string()),
        }
    }

    /// Build the Rhai scope with policy context variables.
    fn build_scope<'a>(&self, ctx: &'a PolicyContext) -> Scope<'a> {
        let mut scope = Scope::new();

        // Subject Map
        let mut subject_map = rhai::Map::new();
        subject_map.insert("id".into(), Dynamic::from(ctx.subject.id.clone()));
        if let Some(ref iid) = ctx.subject.internal_id {
             subject_map.insert("internal_id".into(), Dynamic::from(iid.clone()));
        }
        // Roles in subject map?
         let roles_array_sub: rhai::Array = ctx.subject.roles
            .iter()
            .map(|r| Dynamic::from(r.clone()))
            .collect();
         subject_map.insert("roles".into(), Dynamic::from(roles_array_sub));
         
        scope.push_constant("subject", subject_map);

        // Subject fields (flat, for simplicity)
        scope.push_constant("subject_id", ctx.subject.id.clone());
        // Convert roles to Dynamic Array for contains() method
        let roles_array: rhai::Array = ctx.subject.roles
            .iter()
            .map(|r| Dynamic::from(r.clone()))
            .collect();
        scope.push_constant("roles", roles_array);

        // Resource Map (simulated object)
        let mut resource_map = rhai::Map::new();
        resource_map.insert("type".into(), Dynamic::from(ctx.resource.resource_type.clone()));
        resource_map.insert("id".into(), 
            if let Some(ref id) = ctx.resource.resource_id { Dynamic::from(id.clone()) } else { Dynamic::UNIT }
        );
        
        if let Some(ref owner) = ctx.resource_owner {
             resource_map.insert("owner_id".into(), Dynamic::from(owner.clone()));
        }
        scope.push_constant("resource", resource_map);

        // Keep flat constants for backward compat / ease of use?
        scope.push_constant("resource_type", ctx.resource.resource_type.clone());
        scope.push_constant(
            "resource_id",
            ctx.resource.resource_id.clone().unwrap_or_default(),
        );

        // Operation
        scope.push_constant("op", ctx.op.as_str().to_string());

        // Environment
        scope.push_constant("now", ctx.env.now as i64);

        // Ownership: Explicit System Signal
        let is_owner = match (&ctx.resource_owner, &ctx.subject.internal_id) {
            (Some(res_owner), Some(sub_int)) => res_owner == sub_int,
            _ => false,
        };
        scope.push_constant("owner", is_owner);

        // Input as dynamic (if present)
        if let Some(ref input) = ctx.input {
            if let Ok(map) = json_to_rhai_map(input) {
                scope.push_constant("input", map);
            }
        }

        scope
    }
}

/// Convert JSON object to Rhai map (shallow, for policy use).
fn json_to_rhai_map(value: &serde_json::Value) -> Result<rhai::Map, ()> {
    match value {
        serde_json::Value::Object(obj) => {
            let mut map = rhai::Map::new();
            for (k, v) in obj {
                let key = k.clone().into();
                let val = json_to_dynamic(v);
                map.insert(key, val);
            }
            Ok(map)
        }
        _ => Err(()),
    }
}

/// Convert JSON value to Rhai Dynamic.
fn json_to_dynamic(value: &serde_json::Value) -> Dynamic {
    match value {
        serde_json::Value::Null => Dynamic::UNIT,
        serde_json::Value::Bool(b) => Dynamic::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Dynamic::from(i)
            } else if let Some(f) = n.as_f64() {
                Dynamic::from(f)
            } else {
                Dynamic::UNIT
            }
        }
        serde_json::Value::String(s) => Dynamic::from(s.clone()),
        serde_json::Value::Array(arr) => {
            let vec: Vec<Dynamic> = arr.iter().map(json_to_dynamic).collect();
            Dynamic::from(vec)
        }
        serde_json::Value::Object(obj) => {
            let mut map = rhai::Map::new();
            for (k, v) in obj {
                map.insert(k.clone().into(), json_to_dynamic(v));
            }
            Dynamic::from(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::context::{PolicyEnv, PolicySubject};
    use crate::protocol::Resource;

    fn create_test_context() -> PolicyContext {
        let subject = PolicySubject::new("user-123").with_roles(["admin", "viewer"]);
        let resource = Resource::instance("document", "doc-456");
        let env = PolicyEnv::new(1704067200);
        PolicyContext::new(subject, resource, "resource.read", env)
    }

    // ==================== Basic Evaluation Tests ====================

    #[test]
    fn test_policy_grants_on_true() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate("true", &ctx);
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_policy_denies_on_false() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate("false", &ctx);
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn test_policy_access_subject_id() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate(r#"subject_id == "user-123""#, &ctx);
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_policy_access_roles() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate(r#"roles.contains("admin")"#, &ctx);
        assert_eq!(result.unwrap(), true);

        let result = engine.evaluate(r#"roles.contains("superuser")"#, &ctx);
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn test_policy_access_operation() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate(r#"op == "resource.read""#, &ctx);
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_policy_access_resource() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate(r#"resource_type == "document""#, &ctx);
        assert_eq!(result.unwrap(), true);

        let result = engine.evaluate(r#"resource_id == "doc-456""#, &ctx);
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_policy_access_env_now() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate("now == 1704067200", &ctx);
        assert_eq!(result.unwrap(), true);
    }

    // ==================== Complex Policy Tests ====================

    #[test]
    fn test_policy_owner_or_admin_check() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let policy = r#"
            let is_owner = subject_id == resource_id;
            let is_admin = roles.contains("admin");
            is_owner || is_admin
        "#;

        let result = engine.evaluate(policy, &ctx);
        assert_eq!(result.unwrap(), true); // admin role present
    }

    #[test]
    fn test_policy_read_only_check() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let policy = r#"
            let is_read = op == "resource.read";
            let is_viewer = roles.contains("viewer");
            is_read && is_viewer
        "#;

        let result = engine.evaluate(policy, &ctx);
        assert_eq!(result.unwrap(), true);
    }
    
    #[test]
    fn test_policy_system_owner_keyword() {
        let engine = PolicyEngine::new();
        
        let sub_internal = "internal-123";
        // Case 1: Is Owner (internal IDs match)
        let subject = PolicySubject::new("ext-1").with_internal_id(sub_internal.to_string());
        let resource = Resource::instance("doc", "doc-1");
        let env = PolicyEnv::new(0);
        let ctx = PolicyContext::new(subject, resource, "read", env.clone())
            .with_resource_owner(sub_internal.to_string());
            
        assert!(engine.evaluate("owner", &ctx).unwrap());
        
        // Case 2: Not Owner (mismatch)
        let subject_fail = PolicySubject::new("ext-2").with_internal_id("other-id".to_string());
        let resource_fail = Resource::instance("doc", "doc-1");
        let ctx_fail = PolicyContext::new(subject_fail, resource_fail, "read", env.clone())
            .with_resource_owner(sub_internal.to_string());
            
        assert_eq!(engine.evaluate("owner", &ctx_fail).unwrap(), false);
        
        // Case 3: No Internal ID (provision failure)
        let subject_no_int = PolicySubject::new("ext-3");
        let resource_no = Resource::instance("doc", "doc-1");
        let ctx_no = PolicyContext::new(subject_no_int, resource_no, "read", env)
            .with_resource_owner(sub_internal.to_string());
            
         assert_eq!(engine.evaluate("owner", &ctx_no).unwrap(), false);
    }

    // ==================== Error Handling Tests ====================

    #[test]
    fn test_policy_non_boolean_result_error() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate("42", &ctx);
        assert!(matches!(result, Err(PolicyError::NonBooleanResult(_))));
    }

    #[test]
    fn test_policy_syntax_error() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate("true &&& false", &ctx);
        // Parse errors result in InvalidPolicy (not EvaluationFailed which is for runtime errors)
        assert!(matches!(result, Err(PolicyError::InvalidPolicy(_))));
    }

    #[test]
    fn test_policy_sandbox_violation_loop() {
        let engine = PolicyEngine::new();
        let ctx = create_test_context();

        let result = engine.evaluate("loop { break; }", &ctx);
        // Should be specifically a SandboxViolation (not generic EvaluationFailed)
        assert!(
            matches!(result, Err(PolicyError::SandboxViolation(_))),
            "Expected SandboxViolation, got: {:?}", result
        );
    }

    // ==================== Input Access Tests ====================

    #[test]
    fn test_policy_access_input() {
        let engine = PolicyEngine::new();
        let subject = PolicySubject::new("user-123").with_roles(["admin"]);
        let resource = Resource::instance("document", "doc-456");
        let env = PolicyEnv::new(1704067200);
        let ctx = PolicyContext::new(subject, resource, "resource.update", env)
            .with_input(serde_json::json!({"amount": 100}));

        let result = engine.evaluate("input.amount <= 1000", &ctx);
        assert_eq!(result.unwrap(), true);
    }

    // ==================== Explainability Tests ====================

    #[test]
    fn test_explain_allowed_by_owner() {
        let engine = PolicyEngine::new();
        let internal_id = "internal-user-1";

        let subject = PolicySubject::new("ext-1").with_internal_id(internal_id);
        let resource = Resource::instance("doc", "doc-1");
        let env = PolicyEnv::new(0);
        let ctx = PolicyContext::new(subject, resource, "resource.read", env)
            .with_resource_owner(internal_id);

        // Default policy (None) should allow owner
        let decision = engine.evaluate_with_explanation(None, &ctx);

        assert!(decision.allowed);
        assert!(decision.ownership.is_owner);
        assert_eq!(decision.ownership.column, Some("owner_id".to_string()));
        assert!(matches!(decision.policy.result, PolicyResult::Allow));
        assert!(matches!(decision.policy.script_source, ScriptSource::Default));
        assert_eq!(decision.subject.id, "ext-1");
        assert_eq!(decision.subject.internal_id, Some(internal_id.to_string()));
    }

    #[test]
    fn test_explain_denied_no_ownership() {
        let engine = PolicyEngine::new();

        // Subject with different internal ID than resource owner
        let subject = PolicySubject::new("ext-2").with_internal_id("other-user");
        let resource = Resource::instance("doc", "doc-1");
        let env = PolicyEnv::new(0);
        let ctx = PolicyContext::new(subject, resource, "resource.read", env)
            .with_resource_owner("internal-user-1");

        // Default policy (None) should deny non-owner
        let decision = engine.evaluate_with_explanation(None, &ctx);

        assert!(!decision.allowed);
        assert!(!decision.ownership.is_owner);
        assert_eq!(decision.ownership.column, Some("owner_id".to_string()));
        assert!(matches!(decision.policy.result, PolicyResult::Deny));
    }

    #[test]
    fn test_explain_allowed_by_role() {
        let engine = PolicyEngine::new();

        // Non-owner but has admin role
        let subject = PolicySubject::new("ext-3")
            .with_internal_id("other-user")
            .with_roles(["admin"]);
        let resource = Resource::instance("doc", "doc-1");
        let env = PolicyEnv::new(0);
        let ctx = PolicyContext::new(subject, resource, "resource.read", env)
            .with_resource_owner("internal-user-1");

        // Policy that allows admin bypass
        let policy = r#"owner || roles.contains("admin")"#;
        let decision = engine.evaluate_with_explanation(Some(policy), &ctx);

        assert!(decision.allowed);
        assert!(!decision.ownership.is_owner); // Not owner
        assert!(decision.subject.roles.contains(&"admin".to_string())); // But has admin role
        assert!(matches!(decision.policy.result, PolicyResult::Allow));
        assert!(matches!(decision.policy.script_source, ScriptSource::Inline(_)));
    }

    #[test]
    fn test_explain_policy_error() {
        let engine = PolicyEngine::new();

        let subject = PolicySubject::new("user-1");
        let resource = Resource::instance("doc", "doc-1");
        let env = PolicyEnv::new(0);
        let ctx = PolicyContext::new(subject, resource, "resource.read", env);

        // Invalid policy that returns non-boolean
        let decision = engine.evaluate_with_explanation(Some("42"), &ctx);

        assert!(!decision.allowed);
        assert!(matches!(decision.policy.result, PolicyResult::Error(_)));

        if let PolicyResult::Error(msg) = &decision.policy.result {
            assert!(
                msg.contains("boolean") || msg.contains("i64"),
                "Error should mention type issue: {}", 
                msg
            );
        }
    }

    #[test]
    fn test_explain_captures_operation() {
        let engine = PolicyEngine::new();

        let subject = PolicySubject::new("user-1").with_internal_id("int-1");
        let resource = Resource::instance("document", "doc-1");
        let env = PolicyEnv::new(0);
        let ctx = PolicyContext::new(subject, resource, "document.update", env)
            .with_resource_owner("int-1");

        let decision = engine.evaluate_with_explanation(None, &ctx);

        assert_eq!(decision.op, "document.update");
    }
}
