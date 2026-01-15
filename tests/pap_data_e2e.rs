//! End-to-end tests for PAP Data Pipeline.
//!
//! Tests data.query, data.count, data.insert with Plan-Auth-Plan.

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

// Reuse helpers - copied for standalone test file
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

    let req = Request::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let resp = tower::util::ServiceExt::oneshot(app.clone(), req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    let auth: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    
    (
        auth["token"].as_str().unwrap().to_string(),
        auth["session_id"].as_str().unwrap().to_string(),
    )
}

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
        let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
        println!("Request Fail Body: {:?}", std::str::from_utf8(&bytes));
        return Err(status);
    }

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    Ok(serde_json::from_slice(&bytes).unwrap())
}

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
        println!("Exec fail: {:?}", std::str::from_utf8(&bytes));
        return Err(status);
    }

    let bytes = axum::body::to_bytes(resp.into_body(), 2048).await.unwrap();
    Ok(serde_json::from_slice(&bytes).unwrap())
}

fn create_app() -> (axum::Router, String) {
    let jwt_secret = b"pap-test-secret-32ch";
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://singularity.local",
        "singularity",
        jwt_secret.to_vec(),
    ));

    let policy = PolicyEngine::new();
    let sys_policy = std::fs::read_to_string("src/policy/defaults.rhai").unwrap();
    let signer = CapabilitySigner::new(SigningKey::generate());
    let mut state = SqliteState::in_memory().unwrap();
    singularity::migration::manager::MigrationManager::new().run(&mut state).unwrap();

    let app_state = AppState::new(
        identity, policy, signer, state, sys_policy, jwt_secret.to_vec()
    );
    let code = app_state.get_bootstrap_code_for_test().unwrap();
    (app(app_state), code)
}

#[tokio::test]
async fn test_pap_insert_and_query() {
    let (app, bootstrap_code) = create_app();
    
    // 1. Setup Admin & Model
    let (admin_jwt, _) = register_user(&app, "admin", "pass", Some(&bootstrap_code)).await;
    
    // Promote Admin
    use jsonwebtoken::{decode, encode, Validation, Algorithm, DecodingKey, EncodingKey, Header};
    let decoding_key = DecodingKey::from_secret(b"pap-test-secret-32ch");
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["singularity"]);
    let mut claims = decode::<singularity::identity::StandardClaims>(&admin_jwt, &decoding_key, &validation).unwrap().claims;
    claims.roles = vec!["admin".to_string()];
    let encoding_key = EncodingKey::from_secret(b"pap-test-secret-32ch");
    let admin_jwt = encode(&Header::new(Algorithm::HS256), &claims, &encoding_key).unwrap();

    // Create Model
    let grant = make_request(&app, &admin_jwt, SCHEMA_CREATE_MODEL, Resource::instance("__models", "todo"), Some(json!({"name": "todo"}))).await.unwrap();
    execute_cap(&app, grant["token"].as_str().unwrap(), json!({"name": "todo"})).await.unwrap();

    // Add Fields
    let grant = make_request(&app, &admin_jwt, SCHEMA_ADD_FIELD, Resource::instance("__fields", "f1"), Some(json!({"model_id": "todo", "name": "title", "field_type": { "type": "String" }}))).await.unwrap();
    execute_cap(&app, grant["token"].as_str().unwrap(), json!({"model_id": "todo", "name": "title", "field_type": { "type": "String" }})).await.unwrap();
    
    // 2. Data Insert (PAP)
    let insert_input = json!({
        "rows": [
            { "title": "Buy milk" },
            { "title": "Walk dog" }
        ],
        "returning": ["id", "title"]
    });
    
    let insert_resp = make_request(&app, &admin_jwt, "data.insert", Resource::collection("todo"), Some(insert_input.clone())).await;
    assert!(insert_resp.is_ok(), "Insert request should work");
    
    let grant = insert_resp.unwrap();
    let insert_exec = execute_cap(&app, grant["token"].as_str().unwrap(), insert_input).await;
    assert!(insert_exec.is_ok(), "Insert execution should work");
    let data = insert_exec.unwrap();
    
    // 3. Data Query (PAP)
    let query_input = json!({
        "select": ["id", "title"],
        "order_by": [{ "field": "title", "direction": "asc" }]
    });
    
    let query_resp = make_request(&app, &admin_jwt, "data.query", Resource::collection("todo"), Some(query_input.clone())).await;
    assert!(query_resp.is_ok(), "Query request should work");
    
    let grant = query_resp.unwrap();
    let query_exec = execute_cap(&app, grant["token"].as_str().unwrap(), query_input).await;
    assert!(query_exec.is_ok(), "Query execution should work");
    let rows = query_exec.unwrap();
    
    // Verify rows
    assert!(rows.is_array());
    let arr = rows.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["title"], "Buy milk");
    assert_eq!(arr[1]["title"], "Walk dog");
    
    // 4. Data Count (PAP)
    // Note: QueryInput requires select field even if ignored by count planner
    let count_input = json!({ "select": [] });
    let count_resp = make_request(&app, &admin_jwt, "data.count", Resource::collection("todo"), Some(count_input.clone())).await;
    assert!(count_resp.is_ok());
    let grant = count_resp.unwrap();
    let count_exec = execute_cap(&app, grant["token"].as_str().unwrap(), count_input).await.unwrap();
    
    assert_eq!(count_exec["count"], 2);
}
