//! End-to-end tests for Access Control.
//!
//! Tests ownership-based access control with real sessions.

use axum::{body::Body, http::{Request, StatusCode}};
use serde_json::json;
use std::sync::Arc;

use singularity::capability::{CapabilitySigner, SigningKey};
use singularity::identity::JwtVerifier;
use singularity::policy::PolicyEngine;
use singularity::protocol::{OpRequest, OpExecute, Resource};
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::http::app;

/// Helper to register a user and get their JWT
async fn register_user(
    app: &axum::Router,
    username: &str,
    password: &str,
) -> (String, String) {
    let register_body = json!({
        "username": username,
        "password": password
    });

    let register_req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), register_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "register should succeed for {}", username);

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    let auth: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    
    (
        auth["token"].as_str().unwrap().to_string(),
        auth["session_id"].as_str().unwrap().to_string(),
    )
}

/// Helper to make an authenticated request
async fn make_request(
    app: &axum::Router,
    jwt: &str,
    op: &str,
    resource: Resource,
    input: Option<serde_json::Value>,
) -> Result<serde_json::Value, StatusCode> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let op_req = OpRequest::new("req-test", op, resource, now)
        .with_input(input.unwrap_or(serde_json::Value::Null));

    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", jwt))
        .body(Body::from(serde_json::to_string(&op_req).unwrap()))
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    
    if resp.status() != StatusCode::OK {
        let status = resp.status();
        // For debugging 500s/403s in tests
        let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
        // Request failed, but we return the status
        return Err(status);
    }

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    Ok(serde_json::from_slice(&bytes).unwrap())
}

/// Helper to execute a capability
async fn execute_cap(
    app: &axum::Router,
    token: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, StatusCode> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let op_exec = OpExecute::new(
        "exec-test",
        singularity::protocol::CapabilityToken::new(token.to_string()),
        now,
    ).with_payload(payload);

    let req = Request::builder()
        .uri("/v1/op/execute")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&op_exec).unwrap()))
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    
    if resp.status() != StatusCode::OK {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
        // Execution failed, but we return the status
        return Err(status);
    }

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    Ok(serde_json::from_slice(&bytes).unwrap())
}

fn create_app() -> axum::Router {
    let jwt_secret = b"access-control-test-secret-32ch";
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://singularity.local",
        "singularity",
        jwt_secret.to_vec(),
    ));

    let policy = PolicyEngine::new();
    let sys_policy = std::fs::read_to_string("src/policy/defaults.rhai").unwrap();
    let signer = CapabilitySigner::new(SigningKey::generate());
    
    let mut state = SqliteState::in_memory().unwrap();
    let migration_manager = singularity::migration::manager::MigrationManager::new();
    migration_manager.run(&mut state).expect("migrations failed");

    let app_state = AppState::new(
        identity,
        policy,
        signer,
        state,
        sys_policy,
        jwt_secret.to_vec(),
    );

    app(app_state)
}

#[tokio::test]
async fn test_provisioning_and_ownership() {
    let app = create_app();

    // 1. Register users: admin, alice, bob
    let (admin_jwt, _) = register_user(&app, "admin", "adminpass123").await;
    let (alice_jwt, _) = register_user(&app, "alice", "alicepass123").await;
    let (bob_jwt, _) = register_user(&app, "bob", "bobpass123").await;

    // Promote Admin: Issue a token with "admin" role
    use jsonwebtoken::{decode, encode, Validation, Algorithm, DecodingKey, EncodingKey, Header};
    let decoding_key = DecodingKey::from_secret(b"access-control-test-secret-32ch");
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["singularity"]);
    
    let token_data = decode::<singularity::identity::StandardClaims>(
        &admin_jwt, 
        &decoding_key, 
        &validation
    ).expect("should decode valid token");
    
    let mut admin_claims = token_data.claims;
    admin_claims.roles = vec!["admin".to_string()];
    
    let encoding_key = EncodingKey::from_secret(b"access-control-test-secret-32ch");
    let admin_jwt = encode(
        &Header::new(Algorithm::HS256),
        &admin_claims,
        &encoding_key
    ).expect("should encode admin token");

    // 2. Admin creates "todo" model
    let create_model = make_request(
        &app,
        &admin_jwt,
        "schema.create_model",
        Resource::instance("__models", "todo"),
        Some(json!({"name": "todo"})),
    ).await;
    
    assert!(create_model.is_ok(), "Admin should create model");
    let grant = create_model.unwrap();
    execute_cap(&app, grant["token"].as_str().unwrap(), json!({"name": "todo"})).await.unwrap();

    // 3. Admin adds "title" field
    let add_field = make_request(
        &app,
        &admin_jwt,
        "schema.add_field",
        Resource::instance("__fields", "field-title"),
        Some(json!({
            "model_id": "todo",
            "name": "title",
            "field_type": { "type": "String" },
            "required": true
        })),
    ).await;
    
    assert!(add_field.is_ok(), "Admin should add field");
    let grant = add_field.unwrap();
    execute_cap(&app, grant["token"].as_str().unwrap(), json!({
        "model_id": "todo",
        "name": "title",
        "field_type": { "type": "String" },
        "required": true
    })).await.unwrap();

    // 4. Alice creates a todo
    let alice_create = make_request(
        &app,
        &alice_jwt,
        "resource.create",
        Resource::instance("todo", "todo-1"),
        Some(json!({"title": "Buy milk"})),
    ).await;
    
    assert!(alice_create.is_ok(), "Alice should create todo");
    let grant = alice_create.unwrap();
    execute_cap(&app, grant["token"].as_str().unwrap(), json!({"title": "Buy milk"})).await.unwrap();

    // 5. Alice reads her own todo (should work - owner)
    let alice_read_grant = make_request(
        &app,
        &alice_jwt,
        "resource.read",
        Resource::instance("todo", "todo-1"),
        None,
    ).await.expect("Alice should get read grant");
    
    let alice_data = execute_cap(&app, alice_read_grant["token"].as_str().unwrap(), json!({})).await.expect("Alice should execute read");
    assert_eq!(alice_data["title"], "Buy milk");

    // 6. Bob tries to read Alice's todo (should be DENIED - not owner)
    let bob_read = make_request(
        &app,
        &bob_jwt,
        "resource.read",
        Resource::instance("todo", "todo-1"),
        None,
    ).await;
    
    assert!(bob_read.is_err(), "Bob should NOT be able to read Alice's todo");
    assert_eq!(bob_read.unwrap_err(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_ownership_hardening() {
    let app = create_app();
    
    // 0. Setup: Create schema (todo model) as Admin
    let (admin_jwt, _) = register_user(&app, "admin", "adminpass123").await;
    
    // Promote to Admin (Forge Token)
    use jsonwebtoken::{decode, encode, Validation, Algorithm, DecodingKey, EncodingKey, Header};
    let decoding_key = DecodingKey::from_secret(b"access-control-test-secret-32ch");
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["singularity"]);
    let token_data = decode::<singularity::identity::StandardClaims>(&admin_jwt, &decoding_key, &validation).unwrap();
    let mut admin_claims = token_data.claims;
    admin_claims.roles = vec!["admin".to_string()];
    let encoding_key = EncodingKey::from_secret(b"access-control-test-secret-32ch");
    let admin_jwt = encode(&Header::new(Algorithm::HS256), &admin_claims, &encoding_key).unwrap();

    // Create Model "todo"
    let create_model = make_request(
        &app,
        &admin_jwt,
        "schema.create_model",
        Resource::instance("__models", "todo"),
        Some(json!({"name": "todo"})),
    ).await;
    assert!(create_model.is_ok(), "Admin should create model");
    execute_cap(&app, create_model.unwrap()["token"].as_str().unwrap(), json!({"name": "todo"})).await.unwrap();

    // Add "title" field
    let add_title = make_request(
        &app,
        &admin_jwt,
        "schema.add_field",
        Resource::instance("__fields", "field-title"),
        Some(json!({
            "model_id": "todo",
            "name": "title",
            "field_type": { "type": "String" },
            "required": true
        })),
    ).await;
    assert!(add_title.is_ok(), "Admin should add title field");
    execute_cap(&app, add_title.unwrap()["token"].as_str().unwrap(), json!({
        "model_id": "todo",
        "name": "title",
        "field_type": { "type": "String" },
        "required": true
    })).await.unwrap();

    // Users
    let (alice_jwt, _) = register_user(&app, "alice", "alicepass123").await;

    // 1. Source of Truth: Alice tries to spoof owner_id on CREATE
    let spoof_val = "fake-owner-id";
    let create_req = make_request(
        &app,
        &alice_jwt,
        "resource.create",
        Resource::instance("todo", "todo-spoof"),
        Some(json!({
            "title": "Spoof Attempt",
            "owner_id": spoof_val 
        })),
    ).await;
    
    assert!(create_req.is_ok(), "Create should be allowed despite spoof attempt");
    let grant = create_req.unwrap();
    execute_cap(&app, grant["token"].as_str().unwrap(), json!({
        "title": "Spoof Attempt",
        "owner_id": spoof_val
    })).await.expect("Execution should succeed");

    // Verify Owner ID is Alice's internal ID, NOT spoof_val
    let read_grant = make_request(
        &app,
        &alice_jwt,
        "resource.read",
        Resource::instance("todo", "todo-spoof"),
        None,
    ).await.unwrap();
    
    let alice_data = execute_cap(&app, read_grant["token"].as_str().unwrap(), json!({})).await.unwrap();
    let actual_owner = alice_data["owner_id"].as_str().expect("owner_id should exist in record");
    
    assert_ne!(actual_owner, spoof_val, "Owner ID should NOT be spoofed input");
    assert!(actual_owner.len() > 10, "Owner ID should be a valid internal ID (got {})", actual_owner);

    // 2. Immutability: Alice tries to change owner_id on UPDATE
    let update_req = make_request(
        &app,
        &alice_jwt,
        "resource.update",
        Resource::instance("todo", "todo-spoof"),
        Some(json!({
            "owner_id": "bob-target-id"
        })),
    ).await;
    
    assert!(update_req.is_ok(), "Alice should be allowed to call update");
    let grant = update_req.unwrap();
    execute_cap(&app, grant["token"].as_str().unwrap(), json!({
        "owner_id": "bob-target-id"
    })).await.unwrap();
    
    // Verify Owner ID is STILL Alice (unchanged)
    let read_again_grant = make_request(
        &app,
        &alice_jwt,
        "resource.read",
        Resource::instance("todo", "todo-spoof"),
        None,
    ).await.unwrap();
    
    let read_data = execute_cap(&app, read_again_grant["token"].as_str().unwrap(), json!({})).await.unwrap();
    let current_owner = read_data["owner_id"].as_str().unwrap();
    assert_eq!(current_owner, actual_owner, "Owner ID should be immutable");
    assert_ne!(current_owner, "bob-target-id");
}
