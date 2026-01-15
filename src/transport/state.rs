//! Shared Application Config/State.
//!
//! Holds the kernel components.
//!
//! # Invariants
//!
//! - `AppState::new()` must be called AFTER migrations have run.
//!   The bootstrap detection queries `__internal_users` which must exist.

use std::sync::{Arc, RwLock};

use crate::capability::CapabilitySigner;
use crate::identity::IdentityVerifier;
use crate::identity::provider::{IdentityService, JwtIssuer};
use crate::policy::PolicyEngine;
use crate::state::SqliteState;

// ============================================================================
// Bootstrap State
// ============================================================================

/// Bootstrap state for first-admin registration.
/// 
/// - Code is generated at startup if no internal users exist.
/// - Code is printed to console once (never exposed via API).
/// - After first admin registration, state is cleared permanently.
#[derive(Debug, Clone)]
pub struct BootstrapState {
    /// Bootstrap code (None = bootstrap complete or never needed).
    pub code: Option<String>,
}

impl BootstrapState {
    /// Generate new bootstrap state with a random code.
    fn new_with_code() -> Self {
        let code = generate_bootstrap_code();
        Self { code: Some(code) }
    }

    /// Create completed bootstrap state (no code needed).
    fn completed() -> Self {
        Self { code: None }
    }
}

/// Generate a random 8-character alphanumeric bootstrap code.
fn generate_bootstrap_code() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // No 0/O/1/I confusion
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

// ============================================================================
// Application State
// ============================================================================

/// Application state shared across all transports.
///
/// Must be efficiently clonable (Arc everything).
#[derive(Clone)]
pub struct AppState {
    /// Identity verification mechanism.
    pub identity: Arc<dyn IdentityVerifier>,
    /// Policy evaluation engine.
    pub policy: Arc<PolicyEngine>,
    /// Capability token signer.
    pub signer: Arc<CapabilitySigner>,
    /// Persistence layer (SQLite).
    pub state: Arc<SqliteState>,
    /// System-wide policy script.
    pub system_policy: String,
    /// Identity provider (for login/register).
    identity_service: Arc<IdentityService>,
    /// Bootstrap state (synchronized for concurrent access).
    bootstrap: Arc<RwLock<BootstrapState>>,
}

impl AppState {
    /// Create a new application state.
    /// 
    /// # Preconditions
    /// 
    /// Migrations MUST have run before calling this function.
    /// The `__internal_users` table must exist.
    pub fn new(
        identity: Arc<dyn IdentityVerifier>,
        policy: PolicyEngine,
        signer: CapabilitySigner,
        state: SqliteState,
        system_policy: String,
        jwt_secret: Vec<u8>,
    ) -> Self {
        // Create JWT issuer with same config as verifier
        let jwt_issuer = JwtIssuer::new(
            jwt_secret,
            "https://singularity.local",
            "singularity",
        );
        
        let identity_service = IdentityService::new(jwt_issuer);

        // Determine bootstrap state
        let has_users = state.has_any_internal_user().unwrap_or(false);
        let bootstrap = if has_users {
            BootstrapState::completed()
        } else {
            let bs = BootstrapState::new_with_code();
            if let Some(ref code) = bs.code {
                tracing::warn!("========================================");
                tracing::warn!("BOOTSTRAP ADMIN CODE: {}", code);
                tracing::warn!("Use this code to register the first admin.");
                tracing::warn!("This code will NOT be shown again.");
                tracing::warn!("========================================");
            }
            bs
        };
        
        Self {
            identity,
            policy: Arc::new(policy),
            signer: Arc::new(signer),
            state: Arc::new(state),
            system_policy,
            identity_service: Arc::new(identity_service),
            bootstrap: Arc::new(RwLock::new(bootstrap)),
        }
    }

    /// Get the identity service for auth operations.
    pub fn identity_service(&self) -> &IdentityService {
        &self.identity_service
    }

    /// Get the SQLite state for direct access.
    pub fn sqlite_state(&self) -> &SqliteState {
        &self.state
    }

    // ========================================================================
    // System Lifecycle
    // ========================================================================

    /// Check if the system bootstrap is complete (at least one internal user exists).
    /// 
    /// This is a semantic wrapper — SqliteState knows about users,
    /// AppState knows about bootstrap lifecycle.
    pub fn is_bootstrap_complete(&self) -> bool {
        let bs = self.bootstrap.read().unwrap();
        bs.code.is_none()
    }

    /// Verify a bootstrap code is correct.
    /// 
    /// Returns true if:
    /// - System is in bootstrap mode AND
    /// - The provided code matches the generated code
    pub fn verify_bootstrap_code(&self, code: &str) -> bool {
        let bs = self.bootstrap.read().unwrap();
        match &bs.code {
            Some(expected) => expected == code,
            None => false, // Not in bootstrap mode
        }
    }

    /// Complete the bootstrap process.
    /// 
    /// Clears the bootstrap code permanently. After this call:
    /// - `is_bootstrap_complete()` returns true
    /// - `verify_bootstrap_code()` always returns false
    pub fn complete_bootstrap(&self) {
        let mut bs = self.bootstrap.write().unwrap();
        bs.code = None;
    }

    /// Get the bootstrap code (for testing only).
    /// 
    /// # Warning
    /// 
    /// This method is for E2E tests only. Production code should never
    /// expose the bootstrap code except via console log at startup.
    pub fn get_bootstrap_code_for_test(&self) -> Option<String> {
        let bs = self.bootstrap.read().unwrap();
        bs.code.clone()
    }
}
