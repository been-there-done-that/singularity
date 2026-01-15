//! Shared Application Config/State.
//!
//! Holds the kernel components.

use std::sync::Arc;

use crate::capability::CapabilitySigner;
use crate::identity::IdentityVerifier;
use crate::identity::provider::{IdentityService, JwtIssuer};
use crate::policy::PolicyEngine;
use crate::state::SqliteState;

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
}

impl AppState {
    /// Create a new application state.
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
        
        Self {
            identity,
            policy: Arc::new(policy),
            signer: Arc::new(signer),
            state: Arc::new(state),
            system_policy,
            identity_service: Arc::new(identity_service),
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
        self.state.has_any_internal_user().unwrap_or(false)
    }
}

