//! HTTP Request Handlers.
//!
//! # Dumb Pipe Principle
//!
//! These handlers contain **NO** logic. They only:
//! 1. deserialize bytes
//! 2. call pipeline functions
//! 3. serialize response

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::{OpExecute, OpRequest, CapGrant};
use crate::transport::error::TransportError;
use crate::identity::IdentityError;
use crate::transport::pipeline;
use crate::transport::AppState;

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
pub async fn handle_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<OpRequest>,
) -> Result<Json<CapGrant>, TransportError> {
    let jwt = extract_bearer_token(&headers)?;
    let current_time = now();
    
    let grant = pipeline::process_request(&state, jwt, request, current_time)?;

    Ok(Json(grant))
}

/// POST /v1/op/execute
pub async fn handle_execute(
    State(state): State<AppState>,
    Json(execute): Json<OpExecute>,
) -> Result<Json<serde_json::Value>, TransportError> {
    let current_time = now();

    let result = pipeline::process_execute(&state, execute, current_time)?;

    // Map result to JSON
    // ExecutionResult enum: Read(parsed), Write(affected_count)
    let response = match result {
        crate::execution::ExecutionResult::Read { data } => data,
        crate::execution::ExecutionResult::Write { affected_count } => serde_json::json!({ "rows_affected": affected_count }),
        crate::execution::ExecutionResult::NoOp => serde_json::json!({ "status": "no-op" }),
    };

    Ok(Json(response))
}
