//! HTTP Server configuration and routing.

use axum::{
    routing::post,
    Router,
};
use tower_http::trace::TraceLayer;

use crate::transport::AppState;
use super::handlers::{handle_execute, handle_request};

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
