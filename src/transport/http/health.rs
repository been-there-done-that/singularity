//! Health check endpoint.
//!
//! Reports system readiness including bootstrap state.
//! This endpoint is unauthenticated and safe to expose publicly.

use axum::{
    extract::State,
    Json,
};
use serde::Serialize;

use crate::transport::AppState;

/// Health response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Overall system status.
    pub status: &'static str,
    /// Individual health checks.
    pub checks: HealthChecks,
}

/// Individual health check results.
#[derive(Debug, Serialize)]
pub struct HealthChecks {
    /// Database connectivity.
    pub db: &'static str,
    /// Bootstrap state.
    pub bootstrap: &'static str,
}

/// GET /healthz — System readiness check.
///
/// Returns:
/// - `status`: "ok" | "bootstrapping" | "degraded"
/// - `checks.db`: "ok" | "error"
/// - `checks.bootstrap`: "complete" | "required" | "unknown"
///
/// This endpoint is:
/// - Unauthenticated
/// - Read-only
/// - Never panics
/// - Safe to expose publicly
pub async fn handle_healthz(State(app): State<AppState>) -> Json<HealthResponse> {
    // Check DB connectivity
    let db_ok = app.sqlite_state().ping().is_ok();

    // Check bootstrap state (only if DB is ok)
    let (bootstrap, bootstrap_complete) = if db_ok {
        if app.is_bootstrap_complete() {
            ("complete", true)
        } else {
            ("required", false)
        }
    } else {
        ("unknown", false)
    };

    // Determine overall status
    let status = match (db_ok, bootstrap_complete) {
        (true, true) => "ok",
        (true, false) => "bootstrapping",
        (false, _) => "degraded",
    };

    Json(HealthResponse {
        status,
        checks: HealthChecks {
            db: if db_ok { "ok" } else { "error" },
            bootstrap,
        },
    })
}
