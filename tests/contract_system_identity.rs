use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use singularity::capability::{CapabilitySigner, SigningKey};
use singularity::identity::JwtVerifier;
use singularity::policy::PolicyEngine;
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::http::app;

// --- Helpers Inlined ---

async fn setup_app() -> (axum::Router, String) {
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

async fn register_admin(app: axum::Router, bootstrap_code: &str, username: &str, password: &str) -> String {
    let register_body = json!({
        "username": username,
        "password": password,
        "bootstrap_code": bootstrap_code
    });

    let req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app, req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["token"].as_str().unwrap().to_string()
}

// --- Data Structures ---

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    // checks: Option<Value>, 
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthResponse {
    token: String,
    user_id: String,
    // expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct SessionResponse {
    valid: bool,
    expires_in: Option<u64>,
}

// --- Tests ---

#[tokio::test]
async fn test_system_health_contract() {
    let (app, _) = setup_app().await;
    
    // 1. Check healthz contract (Unauthenticated)
    let req = Request::builder()
        .uri("/healthz")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app, req).await.unwrap();
    assert_eq!(resp.status(), 200);
    
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    
    // Assert Strict Shape
    assert!(body.get("status").is_some());
    // assert!(body.get("version").is_some()); 
    
    // Check status values
    let status = body.get("status").unwrap().as_str().unwrap();
    assert!(["ok", "bootstrapping", "degraded"].contains(&status));
}

#[tokio::test]
async fn test_bootstrap_state_contract() {
    // 1. Start fresh (bootstrap mode)
    let (app, code) = setup_app().await;
    // Verify bootstrapping status
    let req = Request::builder()
        .uri("/healthz")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    
    assert_eq!(body["status"], "bootstrapping");
    assert_eq!(body["checks"]["bootstrap"], "required");

    // 2. Register Admin (Complete Bootstrap)
    let _token = register_admin(app.clone(), &code, "admin_boot", "password123").await;
    
    // Verify status changed to ok
    let req = Request::builder()
        .uri("/healthz")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app, req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(body["status"], "ok");
    assert_eq!(body["checks"]["bootstrap"], "complete");
}

#[tokio::test]
async fn test_auth_lifecycle_contract() {
    let (app, code) = setup_app().await;
    
    // 1. Login Contract
    // First register
    let _ = register_admin(app.clone(), &code, "lifecycle_user", "pass").await;
    
    // Then login
     let req = Request::builder()
        .uri("/auth/login")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "username": "lifecycle_user",
            "password": "pass"
        }).to_string()))
        .unwrap();
        
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), 200);
    
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let auth_data: AuthResponse = serde_json::from_slice(&bytes).unwrap();
    
    assert!(auth_data.user_id.starts_with("auth:"));

    // 2. Session Status (Valid)
    let req = Request::builder()
        .uri("/auth/session")
        .method("GET")
        .header("Authorization", format!("Bearer {}", auth_data.token))
        .body(Body::empty())
        .unwrap();
        
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), 200);
    
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let session: SessionResponse = serde_json::from_slice(&bytes).unwrap();
    assert!(session.valid);
    assert!(session.expires_in.is_some());

    // 3. Logout Contract
    let req = Request::builder()
        .uri("/auth/logout")
        .method("POST")
        .header("Authorization", format!("Bearer {}", auth_data.token))
        .body(Body::empty())
        .unwrap();
        
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), 204);

    // 4. Verify Revocation (Session Check)
    let req = Request::builder()
        .uri("/auth/session")
        .method("GET")
        .header("Authorization", format!("Bearer {}", auth_data.token))
        .body(Body::empty())
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), 200);
    
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let check_body: SessionResponse = serde_json::from_slice(&bytes).unwrap();
    assert!(!check_body.valid); 

    // 5. Verify Revocation (Auth Me)
    let req = Request::builder()
        .uri("/auth/me")
        .method("GET")
        .header("Authorization", format!("Bearer {}", auth_data.token))
        .body(Body::empty())
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), 401); 
}
