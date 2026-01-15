//! Shared pipeline logic for all transports.
//!
//! # Source of Truth
//!
//! This module defines the canonical behavior for `Request` and `Execute` operations.
//! Both HTTP and gRPC handlers MUST call these functions.

use crate::execution::{ExecutionContext, ExecutionMeta, ExecutionTarget, StateBackedExecutor, OperationExecutor, ExecutionResult};
use crate::policy::{PolicyContext, PolicyEnv};
use crate::protocol::{CapabilityPayload, FieldSet, OpExecute, OpRequest, CapGrant};
use crate::transport::error::TransportError;
use crate::transport::AppState;
use crate::state::State;

/// Core logic for `Request` operation.
///
/// Pipeline:
/// 1. Identity Verification (JWT)
/// 2. Session Verification (skh check, revocation check)
/// 3. Policy Evaluation
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

    let allowed = app.policy.evaluate(&app.system_policy, &policy_ctx)
        .map_err(|_| TransportError::Internal("policy evaluation failed".into()))?;

    if !allowed {
        return Err(TransportError::PolicyDenied);
    }

    // 3. Capability Minting
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
    };

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
