//! Shared Application Config/State.
//!
//! Holds the kernel components.

use std::sync::Arc;

use crate::capability::CapabilitySigner;
use crate::identity::IdentityVerifier;
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
}

impl AppState {
    /// Create a new application state.
    pub fn new(
        identity: Arc<dyn IdentityVerifier>,
        policy: PolicyEngine,
        signer: CapabilitySigner,
        state: SqliteState,
        system_policy: String,
    ) -> Self {
        Self {
            identity,
            policy: Arc::new(policy),
            signer: Arc::new(signer),
            state: Arc::new(state),
            system_policy,
        }
    }
}
