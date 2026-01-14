//! Transport layer errors.
//!
//! Maps kernel errors to HTTP status codes.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

use crate::execution::ExecutionError;
use crate::identity::IdentityError;
use crate::state::StateError;

/// Errors that can occur at the transport layer.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Identity verification failed (401).
    #[error("unauthorized: {0}")]
    Unauthorized(#[from] IdentityError),

    /// Policy denied the operation (403).
    #[error("policy denied operation")]
    PolicyDenied,

    /// Capability extraction failed (400/401).
    #[error("invalid capability: {0}")]
    InvalidCapability(String),

    /// Execution error (mapped to various codes).
    #[error("execution error: {0}")]
    Execution(#[from] ExecutionError),

    /// Internal server error (500).
    #[error("device error: {0}")]
    Internal(String),

    /// Bad request (400).
    #[error("bad request: {0}")]
    BadRequest(String),

    /// State error.
    #[error("state error: {0}")]
    State(#[from] StateError),
}

impl IntoResponse for TransportError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            TransportError::Unauthorized(e) => (StatusCode::UNAUTHORIZED, e.to_string()),
            TransportError::PolicyDenied => (StatusCode::FORBIDDEN, "policy denied".to_string()),
            TransportError::InvalidCapability(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            TransportError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            TransportError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            TransportError::Execution(e) => match e {
                ExecutionError::ResourceNotFound { .. } => (StatusCode::NOT_FOUND, e.to_string()),
                ExecutionError::ConstraintViolation { .. } => (StatusCode::CONFLICT, e.to_string()),
                ExecutionError::UnauthorizedFieldWrite { .. } => {
                    (StatusCode::FORBIDDEN, e.to_string())
                }
                ExecutionError::BackendCapabilityMissing(_) => {
                    (StatusCode::NOT_IMPLEMENTED, e.to_string())
                }
                ExecutionError::ResourceMismatch { .. } => (StatusCode::FORBIDDEN, e.to_string()),
                ExecutionError::StorageError(_) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
                ExecutionError::OperationNotSupported(_) => {
                    (StatusCode::BAD_REQUEST, e.to_string())
                }
                ExecutionError::BadRequest(_) => (StatusCode::BAD_REQUEST, e.to_string()),
            },
            TransportError::State(e) => match e {
                StateError::NotFound { .. } => (StatusCode::NOT_FOUND, e.to_string()),
                StateError::ConstraintViolation { .. } => (StatusCode::CONFLICT, e.to_string()),
                StateError::CapabilityNotSupported(_) => (StatusCode::NOT_IMPLEMENTED, e.to_string()),
                StateError::BadRequest(_) => (StatusCode::BAD_REQUEST, e.to_string()),
                StateError::ConnectionError(_) | StateError::InternalError(_) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                }
            },
        };

        let body = Json(json!({
            "error": message
        }));

        (status, body).into_response()
    }
}
