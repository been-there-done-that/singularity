//! HTTP Server configuration and routing.

use axum::{
    routing::post,
    Router,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::capability::CapabilitySigner;
use crate::identity::IdentityVerifier;
use crate::policy::PolicyEngine;
use crate::state::SqliteState;

use super::handlers::{handle_execute, handle_request};

/// Application state shared across all handlers.
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

/// Create the Axum router.
pub fn app(state: AppState) -> Router {
    Router::new()
        // Intent declaration: JWT -> Capability
        .route("/v1/op/request", post(handle_request))
        // Execution: Capability -> State Change
        .route("/v1/op/execute", post(handle_execute))
        // Observability
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
