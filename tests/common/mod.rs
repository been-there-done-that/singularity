use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt; // Ensure we use this for oneshot

use singularity::capability::{CapabilitySigner, SigningKey};
use singularity::identity::JwtVerifier;
use singularity::policy::PolicyEngine;
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::http::app;

pub async fn setup_app() -> (axum::Router, String) {
    let jwt_secret = b"contract-test-secret-key-32chr";
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://singularity.local",
        "singularity",
        jwt_secret.to_vec(),
    ));

    let policy = PolicyEngine::new();
    let sys_policy = "true"; // Allow all

    let signer = CapabilitySigner::new(SigningKey::generate());
    let mut state = SqliteState::in_memory().unwrap();
    
    let migration_manager = singularity::migration::manager::MigrationManager::new();
    migration_manager.run(&mut state).expect("migrations failed");

    let app_state = AppState::new(
        identity,
        policy,
        signer,
        state,
        sys_policy.to_string(),
        jwt_secret.to_vec(),
    );

    let bootstrap_code = app_state.get_bootstrap_code_for_test().unwrap();
    let app = app(app_state);

    (app, bootstrap_code)
}

pub async fn register_admin(app: axum::Router, bootstrap_code: &str) -> String {
    let register_body = json!({
        "username": "admin",
        "password": "password123",
        "bootstrap_code": bootstrap_code
    });

    let req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["token"].as_str().unwrap().to_string()
}
