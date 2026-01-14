//! HTTP Request Handlers.
//!
//! # Dumb Pipe Principle
//!
//! These handlers contain **NO** logic. They only:
//! 1. deserialize bytes
//! 2. call kernel traits
//! 3. serialize response

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::execution::{ExecutionContext, ExecutionMeta, ExecutionTarget, StateBackedExecutor, OperationExecutor};
use crate::policy::{PolicyContext, PolicyEnv};
use crate::protocol::{CapabilityPayload, FieldSet, OpExecute, OpRequest, CapGrant};
use crate::transport::error::TransportError;
use crate::identity::IdentityError;

use super::server::AppState;

/// Extract JWT from Authorization header.
fn extract_bearer_token(headers: &HeaderMap) -> Result<&str, TransportError> {
    let auth_header = headers
        .get("Authorization")
        .ok_or_else(|| TransportError::Unauthorized(IdentityError::InvalidFormat("missing authorization header".into())))?
        .to_str()
        .map_err(|_| TransportError::Unauthorized(IdentityError::InvalidFormat("invalid header encoding".into())))?;

    if !auth_header.starts_with("Bearer ") {
        return Err(TransportError::Unauthorized(IdentityError::InvalidFormat("missing Bearer prefix".into())));
    }

    Ok(&auth_header[7..])
}

/// Get current timestamp (seconds).
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// POST /v1/op/request
///
/// Pipeline:
/// 1. Identity Verification (JWT) -> PolicySubject
/// 2. Policy Evaluation
/// 3. Capability Minting
pub async fn handle_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<OpRequest>,
) -> Result<Json<CapGrant>, TransportError> {
    // 1. Identity Verification
    let jwt = extract_bearer_token(&headers)?;
    let current_time = now();
    
    // VERIFY: JWT -> Subject
    let subject = state.identity.verify(jwt, current_time)?;

    // 2. Policy Evaluation
    // resource in OpRequest is already the Resource struct, so we can use it directly.
    
    let policy_ctx = PolicyContext::new(
        subject.clone(),
        request.resource.clone(),
        request.op.clone(),
        PolicyEnv::new(current_time),
    ).with_input(request.input.clone().unwrap_or(serde_json::Value::Null));

    // For now, simple "allow all" policy or use a fixed policy? 
    // WAIT: Where does the implementation get the policy SCRIPT from? 
    // In our E2E, we passed the script string to `evaluate`.
    // The `PolicyEngine` doesn't store the policy script text. It stores the *engine*.
    // We need a `PolicyProvider` or store the policy in `AppState`?
    // For v0.1: Hardcoded "System Policy" or simple "all allow" defined in AppState?
    // User didn't specify policy storage. 
    // I'll define a simple rigorous policy in `server.rs` or `lib.rs` and pass it?
    // Actually, `PolicyEngine` has `evaluate(script, context)`.
    // I will assume for now we load a static policy file or use a simple default.
    // I'll add `system_policy: String` to AppState.
    
    // Update: I will modify AppState logic in server.rs to hold the system policy string.
    // But I can't modify `server.rs` easily now without rewriting it.
    // I'll hardcode a strict default policy here for v0.1 or read from env?
    // "Transport is dumb" -> it shouldn't allow all.
    // I'll assume `AppState` has it. I'll modify `AppState` in next step to include `system_policy`.
    
    // For v0.1: Use the system policy from AppState.
    let allowed = state.policy.evaluate(&state.system_policy, &policy_ctx)
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
        current_time,
        current_time + 60, // 60s short lived token
    );

    let token = state.signer.mint(&cap_payload)
        .map_err(|e| TransportError::Internal(e.to_string()))?;

    Ok(Json(CapGrant {
        request_id: request.request_id,
        token,
        expires_at: 60,
    }))
}

/// POST /v1/op/execute
///
/// Pipeline:
/// 1. Capability Verification
/// 2. Execution -> State
pub async fn handle_execute(
    State(state): State<AppState>,
    Json(execute): Json<OpExecute>,
) -> Result<Json<serde_json::Value>, TransportError> {
    // 1. Capability Verification
    // Use signer's verifying key (symmetric trust for now, or use separate verifier)
    // AppState doesn't have CapabilityVerifier. It has CapabilitySigner.
    // CapabilitySigner can produce a VerifyingKey.
    // Ideally AppState should have `verifier: CapabilityVerifier`.
    // I'll use `CapabilityVerifier::new(state.signer.verifying_key())` on the fly.
    
    let verifier = crate::capability::CapabilityVerifier::new(state.signer.verifying_key());
    let current_time = now();

    let verified_cap = verifier.verify(&execute.token, current_time)
        .map_err(|e| TransportError::InvalidCapability(e.to_string()))?;

    // 2. Execution
    let exec_ctx = ExecutionContext::new(verified_cap);
    // Target resource must match capability. Capability has resource. 
    // `OpExecute` doesn't explicitly send resource? 
    // `OpExecute` has `capability_token` and `payload`.
    // The target resource is IMPLICIT in the capability?
    // Wait, `OperationExecutor::execute` takes `target: &ExecutionTarget`.
    // Where do we get the target from?
    // In E2E: `let target = ExecutionTarget::new(Resource::instance(...));`
    // The client currently sends `OpExecute` which JUST has token + payload.
    // The capability HAS the resource in it (`verified_cap.resource`).
    // WE CAN TRUST THE CAPABILITY RESOURCE as the target.
    // Yes! That's the point of capabilities. You don't ask "please write to file X", you present "Token for File X".
    
    let target = ExecutionTarget::new(exec_ctx.resource().clone());
    let meta = ExecutionMeta::new(); // Trace ID etc. can be added later

    // Create ephemeral executor wrapper
    let executor = StateBackedExecutor::new(state.state.as_ref());

    let result = executor.execute(
        &exec_ctx,
        &target,
        &meta,
        execute.payload,
    ).map_err(TransportError::Execution)?;

    // Map result to JSON
    // ExecutionResult enum: Read(parsed), Write(affected_count)
    let response = match result {
        crate::execution::ExecutionResult::Read { data } => data,
        crate::execution::ExecutionResult::Write { affected_count } => json!({ "rows_affected": affected_count }),
        crate::execution::ExecutionResult::NoOp => json!({ "status": "no-op" }),
    };

    Ok(Json(response))
}
