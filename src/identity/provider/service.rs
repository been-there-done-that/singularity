//! Identity Service - orchestrates authentication flows.
//!
//! # Responsibilities
//!
//! - User registration
//! - User login (password verification)
//! - JWT issuance
//!
//! # Non-Responsibilities
//!
//! - Policy evaluation (handled by kernel)
//! - Capability minting (handled by kernel)

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
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
    #[serde(default)]
    pub roles: Vec<String>,
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
    pub user_id: String,
    pub expires_in: u64,
}

/// Identity Service - handles user authentication.
/// 
/// Works directly with SqliteState for auth-specific queries.
pub struct IdentityService {
    jwt_issuer: JwtIssuer,
}

impl IdentityService {
    /// Create a new identity service.
    pub fn new(jwt_issuer: JwtIssuer) -> Self {
        Self { jwt_issuer }
    }

    /// Register a new user.
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

        // 4. Create user
        let user_id = Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Insert into __auth_users
        state.with_connection(|conn| {
            conn.execute(
                "INSERT INTO __auth_users (id, username, email, email_verified, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
                params![&user_id, &req.username, &req.email, now]
            )
        }).map_err(|e| AuthError::Storage(e.to_string()))?;

        // Insert into __auth_secrets
        state.with_connection(|conn| {
            conn.execute(
                "INSERT INTO __auth_secrets (user_id, password_hash, updated_at) VALUES (?1, ?2, ?3)",
                params![&user_id, &password_hash, now]
            )
        }).map_err(|e| AuthError::Storage(e.to_string()))?;

        // 5. Store roles (using existing __internal_users table)
        let roles = if req.roles.is_empty() { vec!["user".to_string()] } else { req.roles.clone() };
        let roles_json = serde_json::to_string(&roles).unwrap_or_else(|_| "[]".to_string());
        
        state.with_connection(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO __internal_users (id, external_subject, roles, status, created_at) VALUES (?1, ?2, ?3, 'active', ?4)",
                params![&user_id, &user_id, &roles_json, now]
            )
        }).map_err(|e| AuthError::Storage(e.to_string()))?;

        // 6. Issue JWT
        let token = self.jwt_issuer
            .issue(&user_id, roles, req.email)
            .map_err(|e| AuthError::Jwt(e.to_string()))?;

        Ok(AuthResponse {
            token,
            user_id,
            expires_in: 30 * 60, // 30 minutes
        })
    }

    /// Login with username and password.
    pub fn login(&self, state: &SqliteState, req: LoginRequest) -> Result<AuthResponse, AuthError> {
        // 1. Find user by username
        let user: Result<(String, Option<String>), AuthError> = state.with_connection(|conn| {
            conn.query_row(
                "SELECT id, email FROM __auth_users WHERE username = ?1 LIMIT 1",
                params![&req.username],
                |row| Ok((row.get(0)?, row.get(1)?))
            ).map_err(|_| AuthError::UserNotFound)
        });

        let (user_id, email) = user?;

        // 2. Get password hash
        let stored_hash: Result<String, AuthError> = state.with_connection(|conn| {
            conn.query_row(
                "SELECT password_hash FROM __auth_secrets WHERE user_id = ?1 LIMIT 1",
                params![&user_id],
                |row| row.get(0)
            ).map_err(|_| AuthError::InvalidCredentials)
        });

        let stored_hash = stored_hash?;

        // 3. Verify password
        if !verify_password(&req.password, &stored_hash)? {
            return Err(AuthError::InvalidCredentials);
        }

        // 4. Get roles from __internal_users
        let roles_json: String = state.with_connection(|conn| {
            conn.query_row(
                "SELECT roles FROM __internal_users WHERE external_subject = ?1 LIMIT 1",
                params![&user_id],
                |row| row.get(0)
            ).unwrap_or_else(|_| "[]".to_string())
        });

        let roles: Vec<String> = serde_json::from_str(&roles_json).unwrap_or_default();

        // 5. Issue JWT
        let token = self.jwt_issuer
            .issue(&user_id, roles, email)
            .map_err(|e| AuthError::Jwt(e.to_string()))?;

        Ok(AuthResponse {
            token,
            user_id,
            expires_in: 30 * 60,
        })
    }
}
