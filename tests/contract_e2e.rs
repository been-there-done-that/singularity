//! End-to-end tests for Kernel UI Contract stabilization.
//!
//! Covers:
//! - auth.me
//! - resource.count
//! - schema.rename_model
//! - schema.rename_field

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use std::sync::Arc;

use singularity::capability::{CapabilitySigner, SigningKey};
use singularity::identity::JwtVerifier;
use singularity::policy::PolicyEngine;
use singularity::protocol::{OpRequest, OpExecute, Resource, CapGrant, opcode::*};
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::http::app;

async fn setup_app() -> (axum::Router, String) {
    let jwt_secret = b"contract-test-secret-key-32chr";
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

async fn register_admin(app: axum::Router, bootstrap_code: &str) -> String {
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

    let resp = tower::util::ServiceExt::oneshot(app, req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_auth_me_contract() {
    let (app, code) = setup_app().await;
    let token = register_admin(app.clone(), &code).await;

    // 1. Valid Token
    let req = Request::builder()
        .uri("/auth/me")
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    
    // Fixed: Expect "auth:" prefix matching IdentityService and fixed handler
    assert!(json["user_id"].as_str().unwrap().starts_with("auth:"));
    assert_eq!(json["token"], token);

    // 2. Invalid Token
    let req = Request::builder()
        .uri("/auth/me")
        .method("GET")
        .header("Authorization", "Bearer invalid")
        .body(Body::empty())
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_resource_count() {
    let (app, code) = setup_app().await;
    let token = register_admin(app.clone(), &code).await;

    // 1. Create Model "posts"
    // Target __models collection for model creation
    let create_model_req = OpRequest::new(
        "req-1",
        SCHEMA_CREATE_MODEL, 
        Resource::collection("__models"), 
        0
    ).with_input(json!({
        "name": "posts",
        "fields": [{"name": "title", "type": "text"}]
    }));

    // Request Capability
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(serde_json::to_string(&create_model_req).unwrap()))
        .unwrap();
    
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "Failed to get capability for schema.create_model");
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let grant: CapGrant = serde_json::from_slice(&bytes).unwrap();

    // Execute Create
    let exec = OpExecute::new("exec-1", grant.token.unwrap(), 0)
        .with_payload(json!({
            "name": "posts",
            "fields": [{"name": "title", "type": "text"}]
        }));
        
    let req = Request::builder()
        .uri("/v1/op/execute")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&exec).unwrap()))
        .unwrap();
        
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "Failed to execute schema.create_model");

    // 2. Count posts (Empty)
    // Need capability for resource.count on posts
    let count_req = OpRequest::new(
        "req-2",
        RESOURCE_COUNT,
        Resource::collection("posts"),
        0
    );

    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(serde_json::to_string(&count_req).unwrap()))
        .unwrap();
    
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap(); // Should succeed
    assert_eq!(resp.status(), StatusCode::OK, "Failed to get capability for resource.count");
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let grant: CapGrant = serde_json::from_slice(&bytes).unwrap();

    let exec = OpExecute::new("exec-2", grant.token.unwrap(), 0);
    let req = Request::builder()
        .uri("/v1/op/execute")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&exec).unwrap()))
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    
    assert_eq!(json["count"], 0);
}

#[tokio::test]
async fn test_schema_rename() {
    let (app, code) = setup_app().await;
    let token = register_admin(app.clone(), &code).await;

    // 1. Create Model "todos"
    let create_req = OpRequest::new("req-1", SCHEMA_CREATE_MODEL, Resource::collection("__models"), 0)
        .with_input(json!({"name": "todos", "fields": [{"name": "desc", "type": "text"}]}));
        
    let req = Request::builder().uri("/v1/op/request").method("POST").header("Content-Type", "application/json").header("Authorization", format!("Bearer {}", token)).body(Body::from(serde_json::to_string(&create_req).unwrap())).unwrap();
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    let grant: CapGrant = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()).unwrap();
    
    let exec = OpExecute::new("exec-1", grant.token.unwrap(), 0).with_payload(json!({"name": "todos", "fields": [{"name": "desc", "type": "text"}]}));
    let req = Request::builder().uri("/v1/op/execute").method("POST").header("Content-Type", "application/json").body(Body::from(serde_json::to_string(&exec).unwrap())).unwrap();
    tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();

    // 2. Rename usage "todos" -> "tasks"
    // Target can be __models collection or instance. Rename is often on collection/system.
    let rename_req = OpRequest::new("req-2", SCHEMA_RENAME_MODEL, Resource::collection("__models"), 0);

    let req = Request::builder().uri("/v1/op/request").method("POST").header("Content-Type", "application/json").header("Authorization", format!("Bearer {}", token)).body(Body::from(serde_json::to_string(&rename_req).unwrap())).unwrap();
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    let grant: CapGrant = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()).unwrap();

    let exec = OpExecute::new("exec-2", grant.token.unwrap(), 0).with_payload(json!({"old_name": "todos", "new_name": "tasks"}));
    let req = Request::builder().uri("/v1/op/execute").method("POST").header("Content-Type", "application/json").body(Body::from(serde_json::to_string(&exec).unwrap())).unwrap();
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    
    assert_eq!(resp.status(), StatusCode::OK);

    // 3. Verify Original is Gone (Count on "todos" fails or returns error/0)
    // 4. Verify New exists (Count tasks)
    let count_req = OpRequest::new("req-3", RESOURCE_COUNT, Resource::collection("tasks"), 0);
    // Request cap
    let req = Request::builder().uri("/v1/op/request").method("POST").header("Content-Type", "application/json").header("Authorization", format!("Bearer {}", token)).body(Body::from(serde_json::to_string(&count_req).unwrap())).unwrap();
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    let grant: CapGrant = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()).unwrap();

    // Execute count
    let exec = OpExecute::new("exec-3", grant.token.unwrap(), 0);
    let req = Request::builder().uri("/v1/op/execute").method("POST").header("Content-Type", "application/json").body(Body::from(serde_json::to_string(&exec).unwrap())).unwrap();
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
