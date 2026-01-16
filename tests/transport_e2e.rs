//! End-to-end tests for Transport Layer (HTTP).
//!
//! Verifies the full stack:
//! HTTP Request -> Bytes -> Handlers -> Kernel -> State

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;

use singularity::capability::{CapabilitySigner, SigningKey};
use singularity::identity::JwtVerifier;
use singularity::policy::PolicyEngine;
use singularity::protocol::{OpRequest, OpExecute, Resource, CapGrant};
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::http::app;

#[tokio::test]
async fn test_http_transport_flow() {
    // 1. Setup Components
    let jwt_secret = b"transport-test-secret-key-32chr";
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://singularity.local",
        "singularity",
        jwt_secret.to_vec(),
    ));

    let policy = PolicyEngine::new();
    let sys_policy = r#"
        // Allow all for test
        true
    "#;

    let signer = CapabilitySigner::new(SigningKey::generate());

    let mut state = SqliteState::in_memory().unwrap();
    
    // Run migrations
    let migration_manager = singularity::migration::manager::MigrationManager::new();
    migration_manager.run(&mut state).expect("migrations failed");

    // App State
    let app_state = AppState::new(
        identity,
        policy,
        signer,
        state,
        sys_policy.to_string(),
        jwt_secret.to_vec(),
    );

    // Get bootstrap code for first registration
    let bootstrap_code = app_state.get_bootstrap_code_for_test().unwrap();

    let app = app(app_state);

    // 2. Register a user (this creates a real session)
    let register_body = json!({
        "username": "testuser",
        "password": "testpassword123",
        "device_name": "e2e-test",
        "bootstrap_code": bootstrap_code
    });

    let register_req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let register_resp = tower::util::ServiceExt::oneshot(app.clone(), register_req).await.unwrap();
    assert_eq!(register_resp.status(), StatusCode::OK);

    let register_bytes = axum::body::to_bytes(register_resp.into_body(), 2048).await.unwrap();
    let auth_response: serde_json::Value = serde_json::from_slice(&register_bytes).unwrap();
    
    // Extract the real JWT (with valid session)
    let jwt = auth_response["token"].as_str().unwrap();

    // 3. Request Capability (/v1/op/request)
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let op_req = OpRequest::new(
        "req-1",
        "resource.create",
        Resource::instance("doc", "http-1"),
        now,
    ).with_input(json!({"title": "HTTP Test"}));

    let req_body = serde_json::to_string(&op_req).unwrap();

    let request = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", jwt))
        .body(Body::from(req_body))
        .unwrap();

    let response = tower::util::ServiceExt::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "request should succeed with valid session");

    let body_bytes = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let grant: CapGrant = serde_json::from_slice(&body_bytes).unwrap();
    
    assert!(!grant.token.as_ref().unwrap().as_str().is_empty());
    assert_eq!(grant.request_id, "req-1");

    // 4. Execute Operation (/v1/op/execute)
    let op_exec = OpExecute::new(
        "exec-1",
        grant.token.unwrap(),
        now,
    ).with_payload(json!({"title": "HTTP Test", "content": "Via Axum"}));

    let exec_body = serde_json::to_string(&op_exec).unwrap();

    let execute_req = Request::builder()
        .uri("/v1/op/execute")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(exec_body))
        .unwrap();

    let exec_resp = tower::util::ServiceExt::oneshot(app, execute_req).await.unwrap();
    assert_eq!(exec_resp.status(), StatusCode::OK);
    
    let exec_bytes = axum::body::to_bytes(exec_resp.into_body(), 1024).await.unwrap();
    let result_json: serde_json::Value = serde_json::from_slice(&exec_bytes).unwrap();
    
    // Write returns { "rows_affected": N }
    assert_eq!(result_json["rows_affected"], 1);
}

#[tokio::test]
async fn test_logout_revokes_session() {
    // Setup
    let jwt_secret = b"transport-test-secret-key-32chr";
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://singularity.local",
        "singularity",
        jwt_secret.to_vec(),
    ));

    let policy = PolicyEngine::new();
    let signer = CapabilitySigner::new(SigningKey::generate());
    let mut state = SqliteState::in_memory().unwrap();
    
    let migration_manager = singularity::migration::manager::MigrationManager::new();
    migration_manager.run(&mut state).expect("migrations failed");

    let app_state = AppState::new(
        identity,
        policy,
        signer,
        state,
        "true".to_string(),
        jwt_secret.to_vec(),
    );

    // Get bootstrap code for first registration
    let bootstrap_code = app_state.get_bootstrap_code_for_test().unwrap();

    let app = app(app_state);

    // 1. Register and get JWT
    let register_body = json!({
        "username": "logoutuser",
        "password": "testpassword123",
        "bootstrap_code": bootstrap_code
    });

    let register_req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let register_resp = tower::util::ServiceExt::oneshot(app.clone(), register_req).await.unwrap();
    let register_bytes = axum::body::to_bytes(register_resp.into_body(), 2048).await.unwrap();
    let auth_response: serde_json::Value = serde_json::from_slice(&register_bytes).unwrap();
    
    let jwt = auth_response["token"].as_str().unwrap().to_string();
    let session_id = auth_response["session_id"].as_str().unwrap().to_string();

    // 2. Verify JWT works before logout
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let op_req = OpRequest::new("req-1", "resource.create", Resource::instance("doc", "test"), now)
        .with_input(json!({"x": 1}));

    let pre_logout_req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", jwt))
        .body(Body::from(serde_json::to_string(&op_req).unwrap()))
        .unwrap();

    let pre_logout_resp = tower::util::ServiceExt::oneshot(app.clone(), pre_logout_req).await.unwrap();
    assert_eq!(pre_logout_resp.status(), StatusCode::OK, "should work before logout");

    // 3. Logout
    // let logout_body = json!({ "session_id": session_id });
    let logout_req = Request::builder()
        .uri("/auth/logout")
        .method("POST")
        // .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", jwt))
        .body(Body::empty())
        .unwrap();

    let logout_resp = tower::util::ServiceExt::oneshot(app.clone(), logout_req).await.unwrap();
    assert_eq!(logout_resp.status(), StatusCode::NO_CONTENT);

    // 4. Verify JWT is now rejected
    let post_logout_req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", jwt))
        .body(Body::from(serde_json::to_string(&op_req).unwrap()))
        .unwrap();

    let post_logout_resp = tower::util::ServiceExt::oneshot(app, post_logout_req).await.unwrap();
    assert_eq!(post_logout_resp.status(), StatusCode::UNAUTHORIZED, "should be rejected after logout");
}
