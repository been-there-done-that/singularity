//! HTTP Server configuration and routing.

use axum::{
    routing::post,
    Router,
};
use tower_http::{
    trace::TraceLayer,
    cors::{CorsLayer, Any},
};
use axum::http::Method;

use crate::transport::AppState;
use super::handlers::{handle_execute, handle_request};

/// Create the Axum router.
pub fn app(state: AppState) -> Router {
    // CORS for development - allows frontend on different port
    let cors = CorsLayer::new()
        .allow_origin(Any) // In production, restrict to specific origins
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    Router::new()
        // Intent declaration: JWT -> Capability
        .route("/v1/op/request", post(handle_request))
        // Execution: Capability -> State Change
        .route("/v1/op/execute", post(handle_execute))
        // Observability
        .layer(TraceLayer::new_for_http())
        // CORS
        .layer(cors)
        .with_state(state)
}

