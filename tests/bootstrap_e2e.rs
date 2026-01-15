//! End-to-end tests for Bootstrap Admin Enforcement.
//!
//! Verifies:
//! 1. Fresh + correct code → admin  
//! 2. Fresh + wrong/missing code → fail
//! 3. Post-bootstrap → normal user (code ignored)

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;

use singularity::capability::{CapabilitySigner, SigningKey};
use singularity::identity::JwtVerifier;
use singularity::policy::PolicyEngine;
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::http::app;

/// Helper to create a fresh app state for bootstrap testing.
/// Returns (app, app_state, bootstrap_code)
fn create_bootstrap_app() -> (axum::Router, AppState, String) {
    let jwt_secret = b"bootstrap-test-secret-key-32chr";
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://singularity.local",
        "singularity",
        jwt_secret.to_vec(),
    ));

    let policy = PolicyEngine::new();
    let signer = CapabilitySigner::new(SigningKey::generate());
    let mut state = SqliteState::in_memory().unwrap();
    
    // Run migrations
    let migration_manager = singularity::migration::manager::MigrationManager::new();
    migration_manager.run(&mut state).expect("migrations failed");

    // Create app state - this will generate and log bootstrap code
    let app_state = AppState::new(
        identity,
        policy,
        signer,
        state,
        "true".to_string(),
        jwt_secret.to_vec(),
    );

    // Extract bootstrap code via test method
    let code = app_state.get_bootstrap_code_for_test().unwrap();

    (app(app_state.clone()), app_state, code)
}

// ============================================================================
// Test 1: Fresh system + correct code → admin
// ============================================================================

#[tokio::test]
async fn test_bootstrap_correct_code_succeeds() {
    let (app, app_state, code) = create_bootstrap_app();

    // Verify we're in bootstrap mode
    assert!(!app_state.is_bootstrap_complete());

    let register_body = json!({
        "username": "admin",
        "password": "securepassword123",
        "email": "admin@test.com",
        "bootstrap_code": code
    });

    let register_req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app, register_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "Bootstrap with correct code should succeed");

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let auth_resp: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Should have a token
    assert!(auth_resp["token"].as_str().is_some());
    
    // Bootstrap should now be complete
    assert!(app_state.is_bootstrap_complete());
}

// ============================================================================
// Test 2: Fresh system + wrong/missing code → fail
// ============================================================================

#[tokio::test]
async fn test_bootstrap_wrong_code_fails() {
    let (app, _app_state, _code) = create_bootstrap_app();

    // Test with wrong code
    let register_body = json!({
        "username": "attacker",
        "password": "password123",
        "bootstrap_code": "WRONGCODE"
    });

    let register_req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app, register_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "Wrong code should be rejected");

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let error_resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(error_resp["code"], "BOOTSTRAP_REQUIRED");
}

#[tokio::test]
async fn test_bootstrap_missing_code_fails() {
    let (app, _app_state, _code) = create_bootstrap_app();

    // Test with no code
    let register_body = json!({
        "username": "attacker",
        "password": "password123"
    });

    let register_req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app, register_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "Missing code should be rejected");
}

// ============================================================================
// Test 3: After bootstrap → normal user (code ignored)
// ============================================================================

#[tokio::test]
async fn test_post_bootstrap_registration_succeeds() {
    let (app, app_state, code) = create_bootstrap_app();

    // First: register admin with bootstrap code
    let admin_body = json!({
        "username": "bootstrapadmin",
        "password": "securepassword123",
        "bootstrap_code": code
    });

    let admin_req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&admin_body).unwrap()))
        .unwrap();

    let admin_resp = tower::util::ServiceExt::oneshot(app.clone(), admin_req).await.unwrap();
    assert_eq!(admin_resp.status(), StatusCode::OK, "First admin registration should succeed");

    // Verify bootstrap is complete
    assert!(app_state.is_bootstrap_complete());

    // Second: register normal user (code should be ignored)
    let user_body = json!({
        "username": "normaluser",
        "password": "userpassword123",
        "bootstrap_code": "ANYCODE"  // This should be ignored
    });

    let user_req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&user_body).unwrap()))
        .unwrap();

    let user_resp = tower::util::ServiceExt::oneshot(app, user_req).await.unwrap();
    assert_eq!(user_resp.status(), StatusCode::OK, "Post-bootstrap registration should succeed");
}
