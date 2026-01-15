//! HTTP Server configuration and routing.

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{
    trace::TraceLayer,
    cors::{CorsLayer, Any},
};
use axum::http::Method;

use crate::transport::AppState;
use super::handlers::{handle_execute, handle_request};
use super::auth_handlers::{handle_login, handle_register, handle_logout};
use super::health::handle_healthz;

/// Create the Axum router.
pub fn app(state: AppState) -> Router {
    // CORS for development - allows frontend on different port
    let cors = CorsLayer::new()
        .allow_origin(Any) // In production, restrict to specific origins
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    Router::new()
        // Auth endpoints (no JWT required)
        .route("/auth/login", post(handle_login))
        .route("/auth/register", post(handle_register))
        .route("/auth/logout", post(handle_logout))
        // Intent declaration: JWT -> Capability
        .route("/v1/op/request", post(handle_request))
        // Execution: Capability -> State Change
        .route("/v1/op/execute", post(handle_execute))
        // Health check (unauthenticated)
        .route("/healthz", get(handle_healthz))
        // Observability
        .layer(TraceLayer::new_for_http())
        // CORS
        .layer(cors)
        .with_state(state)
}

