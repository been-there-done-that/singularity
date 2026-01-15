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
use crate::state::State as StateTrait;
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
    
    // SYNC to internal users immediately
    // Note: Roles are not currently returned by login, but ensure_internal_user 
    // will update them when the first authenticated request happens via pipeline.
    // For bootstrap, we focus on registration.
    
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
        tracing::info!(username = %req.username, "Processing registration in BOOTSTRAP mode");
        // Bootstrap mode: require and verify code
        let code_valid = match &req.bootstrap_code {
            Some(code) => {
                let valid = state.verify_bootstrap_code(code);
                tracing::info!(code = %code, valid = %valid, "Bootstrap code verification result");
                valid
            },
            None => {
                tracing::warn!("Bootstrap code missing in request");
                false
            },
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
            .register_with_roles(state.sqlite_state(), req.clone(), roles.clone())
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

        // SYNC to internal users immediately so AppState is aware
        // Use the same subject format as JWTs: "internal:{auth_user_id}"
        let external_subject = format!("internal:{}", response.user_id);
        StateTrait::ensure_internal_user(state.sqlite_state(), &external_subject, &roles)
            .map_err(|e| AuthError::Storage(format!("provisioning failed: {}", e)))?;

        // CRITICAL: Complete bootstrap (clears code permanently)
        state.complete_bootstrap();

        tracing::info!("Bootstrap complete: first admin registered/elevated and provisioned");
        Ok(Json(response.into()))
    } else {
        // Normal registration: user role
        let response = state.identity_service().register(state.sqlite_state(), req)?;
        
        // SYNC to internal users immediately
        let external_subject = format!("internal:{}", response.user_id);
        StateTrait::ensure_internal_user(state.sqlite_state(), &external_subject, &vec!["user".to_string()])
            .map_err(|e| AuthError::Storage(format!("provisioning failed: {}", e)))?;

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

/// GET /auth/me
pub async fn handle_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthResponse>, AuthError> {
    // 1. Extract token
    let auth_header = headers.get("authorization")
        .ok_or(AuthError::InvalidCredentials)?
        .to_str()
        .map_err(|_| AuthError::InvalidCredentials)?;
    
    if !auth_header.starts_with("Bearer ") {
         return Err(AuthError::InvalidCredentials);
    }
    let token = &auth_header[7..];

    // 2. Verify token
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    
    let subject = state.identity.verify(token, now)
        .map_err(|_| AuthError::InvalidCredentials)?; // Return 401 on any verification failure
    
    // 3. Verify Session (DB check)
    let sid = subject.claims.get("sid").and_then(|v| v.as_str()).ok_or(AuthError::InvalidCredentials)?;
    let skh = subject.claims.get("skh").and_then(|v| v.as_str()).ok_or(AuthError::InvalidCredentials)?;
    
    // This checks DB existence and revocation status
    state.identity_service().verify_session(state.sqlite_state(), sid, skh)?;

    // 4. Return Response (echo token)
    let exp = subject.claims.get("exp").and_then(|v| v.as_u64()).unwrap_or(0);
    let expires_in = if exp > now { exp - now } else { 0 };

    Ok(Json(AuthResponse {
        token: token.to_string(),
        user_id: subject.id,
        session_id: sid.to_string(),
        expires_in,
    }))
}

/// Session status response.
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
}

/// GET /auth/session
///
/// Lightweight session validity check.
/// Always returns 200 OK.
pub async fn handle_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<SessionResponse> {
    // 1. Extract token
    let auth_header = match headers.get("authorization").and_then(|h| h.to_str().ok()) {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => return Json(SessionResponse { valid: false, expires_in: None }),
    };

    // 2. Verify token format and signature
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let subject = match state.identity.verify(auth_header, now) {
        Ok(s) => s,
        Err(_) => return Json(SessionResponse { valid: false, expires_in: None }),
    };

    // 3. Verify Session (DB check)
    let sid = match subject.claims.get("sid").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return Json(SessionResponse { valid: false, expires_in: None }),
    };
    let skh = match subject.claims.get("skh").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return Json(SessionResponse { valid: false, expires_in: None }),
    };

    if state.identity_service().verify_session(state.sqlite_state(), sid, skh).is_err() {
        return Json(SessionResponse { valid: false, expires_in: None });
    }

    // 4. Calculate expiry
    let exp = subject.claims.get("exp").and_then(|v| v.as_u64()).unwrap_or(0);
    let expires_in = if exp > now { Some(exp - now) } else { Some(0) };

    Json(SessionResponse {
        valid: true,
        expires_in,
    })
}
