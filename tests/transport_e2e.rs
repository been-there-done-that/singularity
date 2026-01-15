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
// use tower::util::ServiceExt; // Not needed with fully qualified path

use singularity::capability::{CapabilitySigner, SigningKey}; // Check if SigningKey is exported
use singularity::identity::{JwtVerifier, StandardClaims};
use singularity::policy::PolicyEngine;
use singularity::protocol::{OpRequest, OpExecute, Resource, CapGrant};
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::http::app;

#[tokio::test]
async fn test_http_transport_flow() {
    // 1. Setup Components
    // Identity
    let jwt_secret = b"transport-test-secret";
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://transport.test",
        "singularity-transport",
        jwt_secret.to_vec(),
    ));

    // Policy
    let policy = PolicyEngine::new();
    let sys_policy = r#"
        // Allow all for test
        true
    "#;

    // Capability Signer
    let signer = CapabilitySigner::new(SigningKey::generate());

    // State
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

    // Build Router
    let app = app(app_state);

    // 2. Prepare Identity Token (JWT)
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    
    let claims = StandardClaims {
        sub: "user-http".to_string(),
        sid: Some("session-http".to_string()),
        skh: Some("hash-http".to_string()),
        roles: vec!["admin".to_string()],
        groups: vec![],
        email: None,
        name: None,
        iat: Some(now),
        exp: Some(now + 3600),
        nbf: Some(now),
        iss: Some("https://transport.test".to_string()),
        aud: Some(serde_json::json!("singularity-transport")),
    };
    
    let jwt = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(jwt_secret)
    ).unwrap();

    // 3. Request Capability (/v1/op/request)
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
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let grant: CapGrant = serde_json::from_slice(&body_bytes).unwrap();
    
    assert!(!grant.token.as_str().is_empty());
    assert_eq!(grant.request_id, "req-1");

    // 4. Execute Operation (/v1/op/execute)
    let op_exec = OpExecute::new(
        "exec-1",
        grant.token,
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
