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

/// Core logic for `Request` operation.
///
/// Pipeline:
/// 1. Identity Verification
/// 2. Policy Evaluation
/// 3. Capability Minting
pub fn process_request(
    app: &AppState,
    identity_token: &str,
    request: OpRequest,
    now: u64,
) -> Result<CapGrant, TransportError> {
    // 1. Identity Verification
    let subject = app.identity.verify(identity_token, now)?;

    // 2. Policy Evaluation
    // Use resource directly from request (OpRequest owns Resource)
    let policy_ctx = PolicyContext::new(
        subject.clone(),
        request.resource.clone(),
        request.op.clone(),
        PolicyEnv::new(now),
    ).with_input(request.input.clone().unwrap_or(serde_json::Value::Null));

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
