//! Shared pipeline logic for all transports.
//!
//! # Source of Truth
//!
//! This module defines the canonical behavior for `Request` and `Execute` operations.
//! Both HTTP and gRPC handlers MUST call these functions.

use crate::execution::{ExecutionContext, ExecutionMeta, ExecutionTarget, StateBackedExecutor, OperationExecutor, ExecutionResult};
use crate::policy::{AccessDecision, PolicyContext, PolicyEngine, PolicyEnv};
use crate::protocol::{CapabilityPayload, FieldSet, OpExecute, OpRequest, CapGrant};
use crate::transport::error::TransportError;
use crate::transport::AppState;
use crate::state::State;
use crate::planner;
use crate::executor as pap_executor;
use crate::protocol::data::{QueryInput, InsertInput, UpdateInput, DeleteInput, DataAction};
use crate::policy::plan_authorizer::{PlanAuthorizer, PlanAuthContext, ModelPolicyConfig};

// ============================================================================
// Internal Authorization Types (NOT EXPOSED)
// ============================================================================


// ============================================================================
// Internal Authorization Types (NOT EXPOSED)
// ============================================================================

/// Authorization evaluation mode (internal, not exposed).
///
/// The pipeline chooses the mode — callers cannot request it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AuthorizationMode {
    /// Normal path: evaluates and returns bool only
    #[default]
    Enforce,
    /// Debug path: evaluates and captures AccessDecision
    Explain,
}

/// Authorization result with optional explanation.
///
/// This allows the pipeline to optionally capture why a decision was made,
/// without changing the enforcement path.
#[derive(Debug)]
pub(crate) struct AuthorizationResult {
    /// Whether the request was allowed
    pub allowed: bool,
    /// Optional explanation (only populated in Explain mode)
    pub explanation: Option<AccessDecision>,
}

/// Evaluate policy with optional explanation (internal only).
///
/// This is the single integration point for explainability in the pipeline.
/// - In `Enforce` mode: uses `evaluate()` for minimal overhead
/// - In `Explain` mode: uses `evaluate_with_explanation()` and captures decision
fn evaluate_policy(
    engine: &PolicyEngine,
    script: &str,
    ctx: &PolicyContext,
    mode: AuthorizationMode,
) -> Result<AuthorizationResult, TransportError> {
    match mode {
        AuthorizationMode::Enforce => {
            // Normal path: evaluate only, no explanation captured
            let allowed = engine.evaluate(script, ctx)
                .map_err(|_| TransportError::Internal("policy evaluation failed".into()))?;
            Ok(AuthorizationResult {
                allowed,
                explanation: None,
            })
        }
        AuthorizationMode::Explain => {
            // Debug path: capture full decision
            let decision = engine.evaluate_with_explanation(Some(script), ctx);
            Ok(AuthorizationResult {
                allowed: decision.allowed,
                explanation: Some(decision),
            })
        }
    }
}

// ============================================================================
// Pipeline Functions
// ============================================================================


/// Core logic for `Request` operation.
///
/// Pipeline:
/// 1. Identity Verification (JWT)
/// 2. Session Verification (skh check, revocation check)
/// 3. Policy Evaluation (or Planner+Policy for data ops)
/// 4. Capability Minting
pub fn process_request(
    app: &AppState,
    identity_token: &str,
    request: OpRequest,
    now: u64,
) -> Result<CapGrant, TransportError> {
    // 1. Identity Verification (JWT signature + claims)
    let subject = app.identity.verify(identity_token, now)?;

    // 2. Session Verification (skh matches DB, not revoked)
    // Extract session claims from subject
    let sid = subject.claims.get("sid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TransportError::BadRequest("missing session id in token".into()))?;
    let skh = subject.claims.get("skh")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TransportError::BadRequest("missing session key hash in token".into()))?;
    
    // Verify session against database
    app.identity_service().verify_session(app.sqlite_state(), sid, skh)?;

    // 3. User Provisioning
    let internal_id = app.state.ensure_internal_user(&subject.id, &subject.roles)
        .map_err(|e| TransportError::Internal(format!("provisioning failed: {}", e)))?;

    // Enrich subject with internal_id
    let subject = subject.with_internal_id(internal_id);

    // Branch: Data Ops (PAP) vs Resource Ops (Legacy/Simple)
    if request.op.as_str().starts_with("data.") {
        return process_data_request(app, subject, request, now);
    }

    // --- Legacy Resource Pipeline ---

    // 3. Ownership Loading
    let mut policy_ctx = PolicyContext::new(
        subject.clone(),
        request.resource.clone(),
        request.op.clone(),
        PolicyEnv::new(now),
    ).with_input(request.input.clone().unwrap_or(serde_json::Value::Null));

    // If resource is an instance, try to load owner
    if let Some(ref id) = request.resource.resource_id {
        let owner = app.state.get_resource_owner(&request.resource.resource_type, id)
             .map_err(|e| TransportError::Internal(format!("ownership verify failed: {}", e)))?;
        
        if let Some(owner_id) = owner {
            policy_ctx = policy_ctx.with_resource_owner(owner_id);
        }
    }

    // 4. Policy Evaluation
    let auth_result = evaluate_policy(
        &app.policy,
        &app.system_policy,
        &policy_ctx,
        AuthorizationMode::Enforce,
    )?;

    if !auth_result.allowed {
        // First consumer of explainability: structured denial logging
        let explanation = evaluate_policy(
            &app.policy,
            &app.system_policy,
            &policy_ctx,
            AuthorizationMode::Explain,
        ).ok().and_then(|r| r.explanation);
        
        if let Some(decision) = explanation {
            tracing::warn!(
                op = %decision.op,
                subject_id = %decision.subject.id,
                roles = ?decision.subject.roles,
                is_owner = decision.ownership.is_owner,
                policy_result = ?decision.policy.result,
                "Policy denied request"
            );
        }
        
        return Err(TransportError::PolicyDenied);
    }

    // 5. Capability Minting
    // Map requested fields -> FieldSet (default ALL logic for v0.1)
    let fields = FieldSet::all(); 
    
    // Construct payload
    let cap_payload = CapabilityPayload::new(
        &format!("grant-{}", subject.id), // Unique grant ID
        request.op.clone(),
        request.resource.clone(),
        fields,
        now,
        now + 60, // 60s short lived token
    );

    let cap_payload = if let Some(ref iid) = subject.internal_id {
         cap_payload.with_internal_user_id(iid)
    } else {
         cap_payload
    }
    .with_roles(subject.roles.clone());


    let token = app.signer.mint(&cap_payload)
        .map_err(|e| TransportError::Internal(e.to_string()))?;

    Ok(CapGrant::new(
        request.request_id,
        token,
        now + 60,
    ))
}

/// PAP Pipeline for Data Operations
fn process_data_request(
    app: &AppState,
    subject: crate::policy::PolicySubject, 
    request: OpRequest,
    now: u64,
) -> Result<CapGrant, TransportError> {
    use crate::protocol::opcode::*;

    let model_name = &request.resource.resource_type;
    let input_value = request.input.clone().unwrap_or(serde_json::Value::Null);

    // 1. Plan
    // We only support Query and Count for now properly
    // Passing &*app.sqlite_state() which yields &SqliteState (impl SchemaView)
    let (plan, _action) = match request.op.as_str() {
        DATA_QUERY => {
            let input: QueryInput = serde_json::from_value(input_value)
                .map_err(|e| TransportError::BadRequest(format!("invalid query input: {}", e)))?;
            (planner::plan_query(model_name, &input, &*app.sqlite_state())
                .map_err(|e| TransportError::BadRequest(e.to_string()))?, DataAction::Query)
        },
        DATA_COUNT => {
            let input: QueryInput = serde_json::from_value(input_value)
                .map_err(|e| TransportError::BadRequest(format!("invalid query input: {}", e)))?;
            (planner::plan_count(model_name, &input, &*app.sqlite_state())
                .map_err(|e| TransportError::BadRequest(e.to_string()))?, DataAction::Count)
        },
        DATA_INSERT => {
             let mut input: InsertInput = serde_json::from_value(input_value)
                .map_err(|e| TransportError::BadRequest(format!("invalid insert input: {}", e)))?;
             
             // Inject system fields if missing
             for row in &mut input.rows {
                 if let serde_json::Value::Object(map) = row {
                     if !map.contains_key("id") {
                         map.insert("id".into(), serde_json::Value::String(uuid::Uuid::new_v4().to_string()));
                     }
                     if !map.contains_key("created_at") {
                         map.insert("created_at".into(), serde_json::json!(now));
                     }
                     if !map.contains_key("updated_at") {
                         map.insert("updated_at".into(), serde_json::json!(now));
                     }
                     // Map owner_id if not present check?
                     // If subject.internal_id exists, use it.
                     if !map.contains_key("owner_id") {
                         if let Some(iid) = &subject.internal_id {
                              map.insert("owner_id".into(), serde_json::Value::String(iid.clone()));
                         }
                     }
                 }
             }
             (planner::plan_insert(model_name, &input, &*app.sqlite_state())
                .map_err(|e| TransportError::BadRequest(e.to_string()))?, DataAction::Insert)
        },
        // TODO: Update/Delete
        _ => return Err(TransportError::BadRequest(format!("unsupported data op: {}", request.op))),
    };

    // 2. Authorize Plan (Policy)
    // Construct PlanAuthContext
    let auth_ctx = PlanAuthContext {
        subject: subject.clone(),
        now,
        policy_config: ModelPolicyConfig {
            owner_field: Some("owner_id".into()),
            admin_roles: vec!["admin".into()],
            // Default other fields
            ..Default::default()
        }
    };
    
    let authorizer = PlanAuthorizer::new();
    let grant = authorizer.authorize(&plan, &auth_ctx)
        .map_err(|e| TransportError::PolicyDenied)?; 

    // 3. Compute Plan Hash
    let plan_hash = pap_executor::compute_plan_hash(&plan, &grant);

    // 4. Mint Capability
    // Store grant as JSON in constraints
    let constraints = serde_json::to_value(&grant)
        .map_err(|e| TransportError::Internal(format!("failed to serialize grant: {}", e)))?;

    let mut cap_payload = CapabilityPayload::new(
        &format!("grant-{}", subject.id),
        request.op,
        request.resource,
        FieldSet::all(), 
        now,
        now + 60, 
    )
    .with_roles(subject.roles.clone())
    .with_plan_hash(plan_hash)
    .with_constraints(constraints);

    if let Some(ref iid) = subject.internal_id {
         cap_payload = cap_payload.with_internal_user_id(iid);
    }

    let token = app.signer.mint(&cap_payload)
        .map_err(|e| TransportError::Internal(e.to_string()))?;

    Ok(CapGrant::new(
        request.request_id,
        token,
        now + 60,
    ))
}


/// Core logic for `Execute` operation.
///
/// Pipeline:
/// 1. Capability Verification
/// 2. Execution -> State
pub fn process_execute(
    app: &AppState,
    execute: OpExecute,
    now: u64,
) -> Result<ExecutionResult, TransportError> {
    // 1. Capability Verification
    // Use signer's verifying key (symmetric trust for now)
    let verifier = crate::capability::CapabilityVerifier::new(app.signer.verifying_key());

    let verified_cap = verifier.verify(&execute.token, now)
        .map_err(|e| TransportError::InvalidCapability(e.to_string()))?;

    // 2. Execution
    let exec_ctx = ExecutionContext::new(verified_cap);
    
    // Target resource comes from the CAPABILITY itself.
    // Use exec_ctx.resource() which returns &Resource.
    let target = ExecutionTarget::new(exec_ctx.resource().clone());
    let meta = ExecutionMeta::new();

    // Create ephemeral executor wrapper
    let executor = StateBackedExecutor::new(app.state.as_ref());

    let result = executor.execute(
        &exec_ctx,
        &target,
        &meta,
        execute.payload,
    ).map_err(TransportError::Execution)?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{PolicySubject, PolicyResult, ScriptSource};
    use crate::protocol::{Resource, opcode::*};

    #[test]
    fn test_evaluate_policy_enforce_mode() {
        let engine = PolicyEngine::default();
        let subject = PolicySubject::new("user-1").with_internal_id("int-1");
        let resource = Resource::instance("doc", "doc-1");
        let ctx = PolicyContext::new(subject, resource, RESOURCE_READ, PolicyEnv::new(0))
            .with_resource_owner("int-1");

        // Owner should be allowed
        let result = evaluate_policy(
            &engine,
            "owner",
            &ctx,
            AuthorizationMode::Enforce,
        ).unwrap();

        assert!(result.allowed);
        assert!(result.explanation.is_none()); // No explanation in Enforce mode
    }

    #[test]
    fn test_evaluate_policy_explain_mode() {
        let engine = PolicyEngine::default();
        let subject = PolicySubject::new("user-1").with_internal_id("int-1");
        let resource = Resource::instance("doc", "doc-1");
        let ctx = PolicyContext::new(subject, resource, RESOURCE_READ, PolicyEnv::new(0))
            .with_resource_owner("int-1");

        // Owner should be allowed, with explanation
        let result = evaluate_policy(
            &engine,
            "owner",
            &ctx,
            AuthorizationMode::Explain,
        ).unwrap();

        assert!(result.allowed);
        assert!(result.explanation.is_some());

        let explanation = result.explanation.unwrap();
        assert!(explanation.allowed);
        assert!(explanation.ownership.is_owner);
        assert!(matches!(explanation.policy.result, PolicyResult::Allow));
        assert!(matches!(explanation.policy.script_source, ScriptSource::Inline(_)));
    }

    #[test]
    fn test_evaluate_policy_deny_with_explanation() {
        let engine = PolicyEngine::default();
        let subject = PolicySubject::new("user-2").with_internal_id("other-user");
        let resource = Resource::instance("doc", "doc-1");
        let ctx = PolicyContext::new(subject, resource, RESOURCE_READ, PolicyEnv::new(0))
            .with_resource_owner("int-1"); // Different owner

        // Non-owner should be denied
        let result = evaluate_policy(
            &engine,
            "owner",
            &ctx,
            AuthorizationMode::Explain,
        ).unwrap();

        assert!(!result.allowed);
        assert!(result.explanation.is_some());

        let explanation = result.explanation.unwrap();
        assert!(!explanation.allowed);
        assert!(!explanation.ownership.is_owner);
        assert!(matches!(explanation.policy.result, PolicyResult::Deny));
    }
}

