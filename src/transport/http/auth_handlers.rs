//! Auth HTTP handlers.
//!
//! # Endpoints
//!
//! - POST /auth/login - authenticate user
//! - POST /auth/register - create new user
//! - POST /auth/logout - revoke session

use axum::{
    extract::State,
    http::StatusCode,
    Json,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::identity::provider::{
    AuthError, AuthResponse as ServiceAuthResponse, LoginRequest, RegisterRequest,
};
use crate::transport::AppState;

/// HTTP Response for auth endpoints.
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub session_id: String,
    pub expires_in: u64,
}

impl From<ServiceAuthResponse> for AuthResponse {
    fn from(r: ServiceAuthResponse) -> Self {
        Self {
            token: r.token,
            user_id: r.user_id,
            session_id: r.session_id,
            expires_in: r.expires_in,
        }
    }
}

/// Logout request.
#[derive(Debug, Deserialize)]
pub struct LogoutRequest {
    pub session_id: String,
}

/// HTTP Error response.
#[derive(Debug, Serialize)]
pub struct AuthErrorResponse {
    pub error: String,
    pub code: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AuthError::UserNotFound => (StatusCode::UNAUTHORIZED, "USER_NOT_FOUND"),
            AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "INVALID_CREDENTIALS"),
            AuthError::UsernameExists => (StatusCode::CONFLICT, "USERNAME_EXISTS"),
            AuthError::EmailExists => (StatusCode::CONFLICT, "EMAIL_EXISTS"),
            AuthError::SessionRevoked => (StatusCode::UNAUTHORIZED, "SESSION_REVOKED"),
            AuthError::SessionNotFound => (StatusCode::UNAUTHORIZED, "SESSION_NOT_FOUND"),
            AuthError::SessionKeyMismatch => (StatusCode::UNAUTHORIZED, "SESSION_KEY_MISMATCH"),
            AuthError::Password(_) => (StatusCode::INTERNAL_SERVER_ERROR, "PASSWORD_ERROR"),
            AuthError::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "STORAGE_ERROR"),
            AuthError::Jwt(_) => (StatusCode::INTERNAL_SERVER_ERROR, "JWT_ERROR"),
        };

        let body = AuthErrorResponse {
            error: self.to_string(),
            code: code.to_string(),
        };

        (status, Json(body)).into_response()
    }
}

/// POST /auth/login
pub async fn handle_login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AuthError> {
    let response = state.identity_service().login(state.sqlite_state(), req)?;
    Ok(Json(response.into()))
}

/// POST /auth/register
pub async fn handle_register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, AuthError> {
    let response = state.identity_service().register(state.sqlite_state(), req)?;
    Ok(Json(response.into()))
}

/// POST /auth/logout
pub async fn handle_logout(
    State(state): State<AppState>,
    Json(req): Json<LogoutRequest>,
) -> Result<StatusCode, AuthError> {
    state.identity_service().logout(state.sqlite_state(), &req.session_id)?;
    Ok(StatusCode::NO_CONTENT)
}
