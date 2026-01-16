//! End-to-end tests for Access Profiles.
//!
//! Tests CRUD operations for access profiles and scope resolution.

use axum::{body::Body, http::{Request, StatusCode}};
use serde_json::json;
use std::sync::Arc;

use singularity::capability::{CapabilitySigner, SigningKey};
use singularity::identity::JwtVerifier;
use singularity::policy::PolicyEngine;
use singularity::protocol::{OpRequest, OpExecute, Resource, opcode::*};
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::http::app;

/// Helper to register a user and get their JWT
async fn register_user(
    app: &axum::Router,
    username: &str,
    password: &str,
    bootstrap_code: Option<&str>,
) -> (String, String) {
    let register_body = match bootstrap_code {
        Some(code) => json!({
            "username": username,
            "password": password,
            "bootstrap_code": code
        }),
        None => json!({
            "username": username,
            "password": password
        }),
    };

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

/// Helper to make an access.* request (direct result, no execute phase)
async fn access_request(
    app: &axum::Router,
    jwt: &str,
    op: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, StatusCode> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let op_req = OpRequest::new("req-test", op, Resource::collection("__access_profiles"), now)
        .with_input(input);

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
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        eprintln!("ERROR: {} op={} body={}", status, op, body);
        return Err(status);
    }

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    let grant: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    
    // Access ops return direct result in the "result" field
    Ok(grant.get("result").cloned().unwrap_or(serde_json::Value::Null))
}

fn create_app() -> (axum::Router, String) {
    let jwt_secret = b"access-profiles-test-secret-32c";
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

    let bootstrap_code = app_state.get_bootstrap_code_for_test().unwrap();

    (app(app_state), bootstrap_code)
}

// =============================================================================
// Test: Access Profile CRUD Operations
// =============================================================================

#[tokio::test]
async fn test_access_profile_crud() {
    let (app, bootstrap_code) = create_app();

    // 1. Register admin user
    let (admin_jwt, _) = register_user(&app, "admin", "adminpass123", Some(&bootstrap_code)).await;

    // 2. Create a model first (needed for profile)
    let model_input = json!({
        "name": "todos",
        "namespace": "default"
    });
    let op_req = OpRequest::new("req-model", SCHEMA_CREATE_MODEL, Resource::collection("__models"), 
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
        .with_input(model_input);
    
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", admin_jwt))
        .body(Body::from(serde_json::to_string(&op_req).unwrap()))
        .unwrap();
    
    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "model creation should succeed");

    // 3. Create access profile
    let create_result = access_request(&app, &admin_jwt, ACCESS_CREATE_PROFILE, json!({
        "model_id": "todos",
        "principal_type": "role",
        "principal_id": "public",
        "allow_query": true,
        "allow_insert": false,
        "allow_update": false,
        "allow_delete": false,
        "priority": 10,
        "row_scopes": {
            "query": { "type": "all" }
        }
    })).await.expect("create should succeed");

    let profile_id = create_result["id"].as_str().expect("should have id");
    assert!(create_result["created"].as_bool().unwrap_or(false));

    // 4. List profiles
    let list_result = access_request(&app, &admin_jwt, ACCESS_LIST_PROFILES, json!({
        "model_id": "todos"
    })).await.expect("list should succeed");

    assert_eq!(list_result["total"].as_u64().unwrap(), 1);
    assert_eq!(list_result["profiles"][0]["principal_id"].as_str().unwrap(), "public");

    // 5. Get profile
    let get_result = access_request(&app, &admin_jwt, ACCESS_GET_PROFILE, json!({
        "id": profile_id
    })).await.expect("get should succeed");

    assert_eq!(get_result["model_id"].as_str().unwrap(), "todos");
    assert_eq!(get_result["principal_type"].as_str().unwrap(), "role");
    assert!(get_result["allow_query"].as_bool().unwrap());
    assert!(!get_result["allow_insert"].as_bool().unwrap());

    // 6. Update profile
    let update_result = access_request(&app, &admin_jwt, ACCESS_UPDATE_PROFILE, json!({
        "id": profile_id,
        "allow_insert": true,
        "priority": 20
    })).await.expect("update should succeed");

    assert!(update_result["updated"].as_bool().unwrap_or(false));

    // 7. Verify update
    let get_result = access_request(&app, &admin_jwt, ACCESS_GET_PROFILE, json!({
        "id": profile_id
    })).await.expect("get after update should succeed");

    assert!(get_result["allow_insert"].as_bool().unwrap());
    assert_eq!(get_result["priority"].as_u64().unwrap(), 20);

    // 8. Delete profile
    let delete_result = access_request(&app, &admin_jwt, ACCESS_DELETE_PROFILE, json!({
        "id": profile_id
    })).await.expect("delete should succeed");

    assert!(delete_result["deleted"].as_bool().unwrap_or(false));

    // 9. Verify deletion
    let list_result = access_request(&app, &admin_jwt, ACCESS_LIST_PROFILES, json!({
        "model_id": "todos"
    })).await.expect("list after delete should succeed");

    assert_eq!(list_result["total"].as_u64().unwrap(), 0);
}

// =============================================================================
// Test: Admin-Only Enforcement
// =============================================================================

#[tokio::test]
async fn test_access_profile_admin_only() {
    let (app, bootstrap_code) = create_app();

    // 1. Register admin and regular user
    let (_admin_jwt, _) = register_user(&app, "admin", "adminpass123", Some(&bootstrap_code)).await;
    let (user_jwt, _) = register_user(&app, "alice", "alicepass123", None).await;

    // 2. Non-admin should be denied access to create profile
    let result = access_request(&app, &user_jwt, ACCESS_CREATE_PROFILE, json!({
        "model_id": "todos",
        "principal_type": "user",
        "principal_id": "alice",
        "allow_query": true
    })).await;

    assert!(result.is_err(), "non-admin should be denied");
    assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);

    // 3. Non-admin should be denied access to list profiles
    let result = access_request(&app, &user_jwt, ACCESS_LIST_PROFILES, json!({
        "model_id": ""
    })).await;

    assert!(result.is_err(), "non-admin should be denied list");
    assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
}

// =============================================================================
// Test: Duplicate Profile Prevention
// =============================================================================

#[tokio::test]
async fn test_duplicate_profile_rejected() {
    let (app, bootstrap_code) = create_app();
    let (admin_jwt, _) = register_user(&app, "admin", "adminpass123", Some(&bootstrap_code)).await;

    // Create first profile
    let result = access_request(&app, &admin_jwt, ACCESS_CREATE_PROFILE, json!({
        "model_id": "items",
        "principal_type": "role",
        "principal_id": "viewer",
        "allow_query": true
    })).await;
    assert!(result.is_ok(), "first create should succeed");

    // Attempt duplicate (same model + principal_type + principal_id)
    let result = access_request(&app, &admin_jwt, ACCESS_CREATE_PROFILE, json!({
        "model_id": "items",
        "principal_type": "role",
        "principal_id": "viewer",
        "allow_query": false  // different values, but same identity
    })).await;

    assert!(result.is_err(), "duplicate should be rejected");
    // Should return conflict (409) or internal (500) depending on error mapping
}

// =============================================================================
// Test: Safe Defaults (allow_query=true but no scope → Deny)
// =============================================================================

#[tokio::test]
async fn test_safe_defaults_auto_deny() {
    let (app, bootstrap_code) = create_app();
    let (admin_jwt, _) = register_user(&app, "admin", "adminpass123", Some(&bootstrap_code)).await;

    // Create profile with allow_query=true but NO row_scopes
    let result = access_request(&app, &admin_jwt, ACCESS_CREATE_PROFILE, json!({
        "model_id": "products",
        "principal_type": "role",
        "principal_id": "guest",
        "allow_query": true,
        "allow_insert": false,
        "allow_update": false,
        "allow_delete": false
        // NO row_scopes → should auto-insert Deny for query
    })).await.expect("create should succeed");

    let profile_id = result["id"].as_str().unwrap();

    // Get profile and verify scope was auto-inserted as deny
    let profile = access_request(&app, &admin_jwt, ACCESS_GET_PROFILE, json!({
        "id": profile_id
    })).await.expect("get should succeed");

    // The row_scopes should have "query" set to deny as safe default
    // RowScopeType is serialized as {"type": "deny"}
    let scopes = profile.get("row_scopes").and_then(|v| v.as_object());
    assert!(scopes.is_some(), "row_scopes should exist");
    
    if let Some(scopes) = scopes {
        let query_scope = scopes.get("query");
        assert!(query_scope.is_some(), "query scope should exist");
        
        if let Some(scope) = query_scope {
            let scope_type = scope.get("type").and_then(|v| v.as_str());
            assert_eq!(scope_type, Some("deny"), 
                "missing scope should default to deny");
        }
    }
}
