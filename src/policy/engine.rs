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

use super::context::PolicyContext;
use super::error::PolicyError;
use super::sandbox::create_sandboxed_engine;

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
}
