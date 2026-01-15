//! Identity Service - orchestrates authentication flows.
//!
//! # Responsibilities
//!
//! - Registers users (auth identity only)
//! - Verifies credentials
//! - Creates sessions (authority anchors)
//! - Issues JWTs (capability tokens)
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
//!    - Only writes to __auth_users, __auth_secrets, __auth_sessions
//!
//! 3. Session = Authority Anchor (long-lived)
//!    - JWT is short-lived projection bound by `sid` and `skh`
//!
//! 4. Revocation always wins (regardless of JWT exp)

use super::jwt::JwtIssuer;
use super::password::{hash_password, verify_password, PasswordError};
use super::session::{generate_session_key, hash_session_key};
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
    #[error("session revoked")]
    SessionRevoked,
    #[error("session not found")]
    SessionNotFound,
    #[error("session key mismatch")]
    SessionKeyMismatch,
    #[error("bootstrap required")]
    BootstrapRequired,
    #[error("password error: {0}")]
    Password(#[from] PasswordError),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("jwt error: {0}")]
    Jwt(String),
}

/// Request to register a new user.
/// 
/// NOTE: No roles field - roles are determined by bootstrap state.
/// During bootstrap (first user): admin if correct code provided.
/// After bootstrap: always "user".
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
    /// Optional device name for session tracking
    #[serde(default)]
    pub device_name: Option<String>,
    /// Bootstrap code (required for first admin registration)
    #[serde(default)]
    pub bootstrap_code: Option<String>,
}

/// Request to login.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    /// Optional device name for session tracking
    #[serde(default)]
    pub device_name: Option<String>,
}

/// Response containing issued JWT.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthResponse {
    pub token: String,
    /// External subject (auth:<auth_user_id>)
    pub user_id: String,
    /// Session ID (for device management UI)
    pub session_id: String,
    pub expires_in: u64,
}

/// Identity Service - handles user authentication.
/// 
/// ONLY touches __auth_users, __auth_secrets, __auth_sessions tables.
pub struct IdentityService {
    jwt_issuer: JwtIssuer,
}

impl IdentityService {
    /// Create a new identity service.
    pub fn new(jwt_issuer: JwtIssuer) -> Self {
        Self { jwt_issuer }
    }

    /// Build the external subject from an auth user ID.
    fn external_subject(auth_user_id: &str) -> String {
        format!("auth:{}", auth_user_id)
    }

    /// Verify a session is valid.
    ///
    /// Called on EVERY authenticated request. No caching. No shortcuts.
    ///
    /// Checks:
    /// 1. Session exists
    /// 2. Not revoked (revoked_at IS NULL)
    /// 3. skh matches session_key_hash
    pub fn verify_session(
        &self,
        state: &SqliteState,
        session_id: &str,
        session_key_hash: &str,
    ) -> Result<(), AuthError> {
        // 1. Lookup session
        let session: Result<(String, Option<i64>), AuthError> = state.with_connection(|conn| {
            conn.query_row(
                "SELECT session_key_hash, revoked_at FROM __auth_sessions WHERE id = ?1 LIMIT 1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?))
            ).map_err(|_| AuthError::SessionNotFound)
        });

        let (stored_hash, revoked_at) = session?;

        // 2. Check not revoked (revocation always wins)
        if revoked_at.is_some() {
            return Err(AuthError::SessionRevoked);
        }

        // 3. Verify session key hash (proof of possession)
        if stored_hash != session_key_hash {
            return Err(AuthError::SessionKeyMismatch);
        }

        Ok(())
    }

    /// Create a session and issue JWT atomically.
    fn create_session_and_jwt(
        &self,
        state: &SqliteState,
        auth_user_id: &str,
        email: Option<String>,
        device_name: Option<String>,
        roles: Vec<String>,
    ) -> Result<AuthResponse, AuthError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Generate session key and hash
        let session_id = Uuid::new_v4().to_string();
        let session_key = generate_session_key();
        let session_key_hash = hash_session_key(&session_key);

        // Insert session
        state.with_connection(|conn| {
            conn.execute(
                "INSERT INTO __auth_sessions (id, auth_user_id, session_key_hash, device_name, created_at, last_used_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![&session_id, auth_user_id, &session_key_hash, &device_name, now, now]
            )
        }).map_err(|e| AuthError::Storage(e.to_string()))?;

        // Issue JWT with session binding
        let external_sub = Self::external_subject(auth_user_id);

        let token = self.jwt_issuer
            .issue(&external_sub, &session_id, &session_key_hash, roles, email)
            .map_err(|e| AuthError::Jwt(e.to_string()))?;

        Ok(AuthResponse {
            token,
            user_id: external_sub,
            session_id,
            expires_in: 30 * 60,
        })
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

        // 4. Create auth user
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

        // 5. Create session and issue JWT (default to user role)
        self.create_session_and_jwt(state, &auth_user_id, req.email, req.device_name, vec!["user".to_string()])
    }

    /// Register a new user with explicit roles.
    /// 
    /// Used by bootstrap flow to create admin user.
    pub fn register_with_roles(
        &self, 
        state: &SqliteState, 
        req: RegisterRequest,
        roles: Vec<String>,
    ) -> Result<AuthResponse, AuthError> {
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

        // 4. Create auth user
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

        // 5. Create session and issue JWT with specified roles
        self.create_session_and_jwt(state, &auth_user_id, req.email.clone(), req.device_name, roles)
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

        // 4. Create session and issue JWT
        self.create_session_and_jwt(state, &auth_user_id, email, req.device_name, vec!["user".to_string()])
    }

    /// Promote an existing user to admin.
    /// 
    /// Used by bootstrap flow to recover from failed registration where
    /// the user was created but admin role was not assigned.
    pub fn promote_to_admin(&self, state: &SqliteState, req: RegisterRequest) -> Result<AuthResponse, AuthError> {
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

        // 3. Verify password (must be correct to promote)
        if !verify_password(&req.password, &stored_hash)? {
            return Err(AuthError::InvalidCredentials);
        }

        // 4. Create session and issue JWT with admin role
        self.create_session_and_jwt(state, &auth_user_id, email, req.device_name, vec!["admin".to_string()])
    }

    /// Revoke a session (logout).
    pub fn logout(&self, state: &SqliteState, session_id: &str) -> Result<(), AuthError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let rows = state.with_connection(|conn| {
            conn.execute(
                "UPDATE __auth_sessions SET revoked_at = ?1 WHERE id = ?2 AND revoked_at IS NULL",
                params![now, session_id]
            )
        }).map_err(|e| AuthError::Storage(e.to_string()))?;

        if rows == 0 {
            return Err(AuthError::SessionNotFound);
        }

        Ok(())
    }

    /// Revoke all sessions for a user (logout all devices).
    pub fn logout_all(&self, state: &SqliteState, auth_user_id: &str) -> Result<u64, AuthError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let rows = state.with_connection(|conn| {
            conn.execute(
                "UPDATE __auth_sessions SET revoked_at = ?1 WHERE auth_user_id = ?2 AND revoked_at IS NULL",
                params![now, auth_user_id]
            )
        }).map_err(|e| AuthError::Storage(e.to_string()))?;

        Ok(rows as u64)
    }
}
