use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

mod common;
use common::{setup_app, register_admin};

#[tokio::test]
async fn test_capability_contract_request() {
    let (app, code) = setup_app().await;
    let token = register_admin(app.clone(), &code).await;

    // 1. Valid Request -> 200 OK
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({
            "request_id": "req-1",
            "op": "resource.count",
            "resource": { "resource_type": "users", "resource_id": null },
            "timestamp": 0
        }).to_string()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Auth Invalid (No Header) -> 401 Unauthorized
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        // No Auth
        .body(Body::from(json!({
            "request_id": "req-2",
            "op": "resource.count",
            "resource": { "resource_type": "users", "resource_id": null },
            "timestamp": 0
        }).to_string()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED); 

    // 3. Auth Invalid (Bad Token) -> 401 Unauthorized
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer invalid-token")
        .body(Body::from(json!({
            "request_id": "req-3",
            "op": "resource.count",
            "resource": { "resource_type": "users", "resource_id": null },
            "timestamp": 0
        }).to_string()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 4. Bad Payload (Missing Fields) -> 400 Bad Request or 422
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({
            "request_id": "req-4"
            // Missing op, resource
        }).to_string()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status() == StatusCode::UNPROCESSABLE_ENTITY || resp.status() == StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_capability_contract_execute() {
    let (app, code) = setup_app().await;
    let token = register_admin(app.clone(), &code).await;

    // Mint a valid capability
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({
            "request_id": "req-1",
            "op": "resource.count",
            "resource": { "resource_type": "users", "resource_id": null },
            "timestamp": 0
        }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let cap_grant: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let cap_token = cap_grant["token"].as_str().unwrap();

    // 1. Valid Execution -> 200 OK
    let req = Request::builder()
        .uri("/v1/op/execute")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "execute_id": "exec-1",
            "token": cap_token,
            "timestamp": 0
        }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Execution with Bad Cap (Tampered) -> 401 or 403
    let req = Request::builder()
        .uri("/v1/op/execute")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "execute_id": "exec-2",
            "token": "tampered-token",
            "timestamp": 0
        }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN);
}
