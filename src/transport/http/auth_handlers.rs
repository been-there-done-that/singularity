//! Auth HTTP handlers.
//!
//! # Endpoints
//!
//! - POST /auth/login - authenticate user
//! - POST /auth/register - create new user (with bootstrap enforcement)
//! - POST /auth/logout - revoke session

use axum::{
    extract::State,
    http::{StatusCode, HeaderMap},
    Json,
    response::{IntoResponse, Response},
};
use serde::Serialize; 

use crate::identity::IdentityError;
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
            // SECURITY: Same error for both bootstrap states (prevents oracle attacks)
            AuthError::BootstrapRequired => (StatusCode::FORBIDDEN, "BOOTSTRAP_REQUIRED"),
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
/// 
/// # Bootstrap Enforcement
/// 
/// If no internal users exist (bootstrap mode):
/// - Requires `bootstrap_code` field
/// - If code is correct: user becomes admin
/// - If code is wrong/missing: returns 403
/// 
/// After bootstrap:
/// - `bootstrap_code` is ignored
/// - User always gets "user" role
pub async fn handle_register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, AuthError> {
    let is_bootstrap = !state.is_bootstrap_complete();

    if is_bootstrap {
        // Bootstrap mode: require and verify code
        let code_valid = match &req.bootstrap_code {
            Some(code) => state.verify_bootstrap_code(code),
            None => false,
        };

        if !code_valid {
            // Log the attempt server-side
            tracing::warn!(
                username = %req.username,
                has_code = req.bootstrap_code.is_some(),
                "Bootstrap registration failed: invalid or missing code"
            );
            return Err(AuthError::BootstrapRequired);
        }

        // Valid bootstrap code: create admin
        let roles = vec!["admin".to_string()];
        let response = match state.identity_service()
            .register_with_roles(state.sqlite_state(), req.clone(), roles)
        {
            Ok(res) => res,
            Err(AuthError::UsernameExists) => {
                // Recovery: User exists but might not be admin.
                // Try to promote the existing user to admin.
                tracing::info!(username = %req.username, "User exists during bootstrap, attempting promotion to admin");
                state.identity_service().promote_to_admin(state.sqlite_state(), req)?
            }
            Err(e) => return Err(e),
        };

        // CRITICAL: Complete bootstrap (clears code permanently)
        state.complete_bootstrap();

        tracing::info!("Bootstrap complete: first admin registered/elevated");
        Ok(Json(response.into()))
    } else {
        // Normal registration: user role
        let response = state.identity_service().register(state.sqlite_state(), req)?;
        Ok(Json(response.into()))
    }
}

/// POST /auth/logout
pub async fn handle_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AuthError> {
    // 1. Extract token
    let auth_header = headers.get("authorization")
        .ok_or(AuthError::UserNotFound)? // Use appropriate low-level error or ignore
        .to_str()
        .map_err(|_| AuthError::Jwt(IdentityError::InvalidFormat("invalid header chars".into()).to_string()))?;
    
    if !auth_header.starts_with("Bearer ") {
         return Err(AuthError::Jwt(IdentityError::InvalidFormat("missing Bearer scheme".into()).to_string()));
    }
    
    let token = &auth_header[7..];

    // 2. Verify token to get claims
    // We swallow verification errors here - if the token is invalid/expired, we can just 
    // return 204 because the client effectively considers itself logged out.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    
    let subject = match state.identity.verify(token, now) {
        Ok(s) => s,
        Err(_) => return Ok(StatusCode::NO_CONTENT) // Already invalid/expired -> success
    };
    
    // 3. Extract Session ID
    if let Some(sid) = subject.claims.get("sid").and_then(|v| v.as_str()) {
        state.identity_service().logout(state.sqlite_state(), sid)?;
    }
    
    Ok(StatusCode::NO_CONTENT)
}
