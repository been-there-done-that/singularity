//! Identity Service - orchestrates authentication flows.
//!
//! # Responsibilities
//!
//! - Registers users (auth identity only)
//! - Verifies credentials
//! - Issues JWTs
//!
//! # Non-Responsibilities
//!
//! - Policy evaluation (handled by kernel)
//! - Capability minting (handled by kernel)  
//! - Internal user provisioning (handled by pipeline via ensure_internal_user)
//!
//! # CRITICAL INVARIANTS
//!
//! 1. Auth identity != Authorization identity
//!    - JWT sub = "auth:<auth_user_id>" (external subject format)
//!    - Pipeline creates internal_user_id via ensure_internal_user
//!
//! 2. IdentityService NEVER touches __internal_users
//!    - Only writes to __auth_users, __auth_secrets
//!    - Internal user provisioning is pipeline's job

use super::jwt::JwtIssuer;
use super::password::{hash_password, verify_password, PasswordError};
use crate::state::SqliteState;
use thiserror::Error;
use uuid::Uuid;
use rusqlite::params;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("user not found")]
    UserNotFound,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("username already exists")]
    UsernameExists,
    #[error("email already exists")]
    EmailExists,
    #[error("password error: {0}")]
    Password(#[from] PasswordError),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("jwt error: {0}")]
    Jwt(String),
}

/// Request to register a new user.
/// 
/// NOTE: No roles field - users always start as ["user"].
/// Admin promotion is a separate privileged operation.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
}

/// Request to login.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Response containing issued JWT.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthResponse {
    pub token: String,
    /// This is the EXTERNAL subject (auth:<auth_user_id>), not internal_user_id
    pub user_id: String,
    pub expires_in: u64,
}

/// Identity Service - handles user authentication.
/// 
/// Works directly with SqliteState for auth-specific queries.
/// ONLY touches __auth_users and __auth_secrets tables.
pub struct IdentityService {
    jwt_issuer: JwtIssuer,
}

impl IdentityService {
    /// Create a new identity service.
    pub fn new(jwt_issuer: JwtIssuer) -> Self {
        Self { jwt_issuer }
    }

    /// Build the external subject from an auth user ID.
    /// 
    /// This is the canonical format for JWT `sub` claims from password auth.
    /// Format: "auth:<auth_user_id>"
    fn external_subject(auth_user_id: &str) -> String {
        format!("auth:{}", auth_user_id)
    }

    /// Register a new user.
    /// 
    /// Creates auth identity ONLY. Does NOT provision internal user.
    /// The pipeline will provision internal user via ensure_internal_user
    /// when the JWT is first used for a request.
    pub fn register(&self, state: &SqliteState, req: RegisterRequest) -> Result<AuthResponse, AuthError> {
        // 1. Check if username exists
        let exists = state.with_connection(|conn| {
            conn.query_row(
                "SELECT 1 FROM __auth_users WHERE username = ?1 LIMIT 1",
                params![&req.username],
                |_| Ok(true)
            ).unwrap_or(false)
        });
        
        if exists {
            return Err(AuthError::UsernameExists);
        }

        // 2. Check if email exists (if provided)
        if let Some(ref email) = req.email {
            let email_exists = state.with_connection(|conn| {
                conn.query_row(
                    "SELECT 1 FROM __auth_users WHERE email = ?1 LIMIT 1",
                    params![email],
                    |_| Ok(true)
                ).unwrap_or(false)
            });
            
            if email_exists {
                return Err(AuthError::EmailExists);
            }
        }

        // 3. Hash password
        let password_hash = hash_password(&req.password)?;

        // 4. Create auth user (ONLY auth tables, not __internal_users)
        let auth_user_id = Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Insert into __auth_users
        state.with_connection(|conn| {
            conn.execute(
                "INSERT INTO __auth_users (id, username, email, email_verified, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
                params![&auth_user_id, &req.username, &req.email, now]
            )
        }).map_err(|e| AuthError::Storage(e.to_string()))?;

        // Insert into __auth_secrets
        state.with_connection(|conn| {
            conn.execute(
                "INSERT INTO __auth_secrets (user_id, password_hash, updated_at) VALUES (?1, ?2, ?3)",
                params![&auth_user_id, &password_hash, now]
            )
        }).map_err(|e| AuthError::Storage(e.to_string()))?;

        // 5. Build external subject and issue JWT
        // NOTE: Roles are ALWAYS ["user"] for new registrations.
        // Admin promotion is a separate privileged operation.
        let external_sub = Self::external_subject(&auth_user_id);
        let roles = vec!["user".to_string()];
        
        let token = self.jwt_issuer
            .issue(&external_sub, roles, req.email)
            .map_err(|e| AuthError::Jwt(e.to_string()))?;

        Ok(AuthResponse {
            token,
            user_id: external_sub, // Return external subject, not raw auth_user_id
            expires_in: 30 * 60, // 30 minutes
        })
    }

    /// Login with username and password.
    /// 
    /// Returns JWT with external subject. Does NOT touch __internal_users.
    pub fn login(&self, state: &SqliteState, req: LoginRequest) -> Result<AuthResponse, AuthError> {
        // 1. Find user by username
        let user: Result<(String, Option<String>), AuthError> = state.with_connection(|conn| {
            conn.query_row(
                "SELECT id, email FROM __auth_users WHERE username = ?1 LIMIT 1",
                params![&req.username],
                |row| Ok((row.get(0)?, row.get(1)?))
            ).map_err(|_| AuthError::UserNotFound)
        });

        let (auth_user_id, email) = user?;

        // 2. Get password hash
        let stored_hash: Result<String, AuthError> = state.with_connection(|conn| {
            conn.query_row(
                "SELECT password_hash FROM __auth_secrets WHERE user_id = ?1 LIMIT 1",
                params![&auth_user_id],
                |row| row.get(0)
            ).map_err(|_| AuthError::InvalidCredentials)
        });

        let stored_hash = stored_hash?;

        // 3. Verify password
        if !verify_password(&req.password, &stored_hash)? {
            return Err(AuthError::InvalidCredentials);
        }

        // 4. Build external subject and issue JWT
        // NOTE: Roles default to ["user"]. The pipeline's ensure_internal_user
        // will provision/update internal user with actual roles if needed.
        let external_sub = Self::external_subject(&auth_user_id);
        let roles = vec!["user".to_string()];

        let token = self.jwt_issuer
            .issue(&external_sub, roles, email)
            .map_err(|e| AuthError::Jwt(e.to_string()))?;

        Ok(AuthResponse {
            token,
            user_id: external_sub, // Return external subject, not raw auth_user_id
            expires_in: 30 * 60,
        })
    }
}
