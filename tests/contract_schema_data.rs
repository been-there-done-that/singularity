use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

mod common;
use common::{setup_app, register_admin};

#[tokio::test]
async fn test_schema_describe_contracts() {
    let (app, code) = setup_app().await;
    let token = register_admin(app.clone(), &code).await;

    // 1. Create a model "products" with fields
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({
            "request_id": "req-create",
            "op": "schema.create_model", 
            "resource": { "resource_type": "__models", "resource_id": null }, 
            "input": {
                "name": "products",
                "fields": [
                    {"name": "sku", "type": "text"},
                    {"name": "price", "type": "integer"}
                ]
            },
            "timestamp": 0
        }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let grant: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let cap_token = grant["token"].as_str().unwrap();

    let req = Request::builder()
        .uri("/v1/op/execute")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "execute_id": "exec-create",
            "token": cap_token,
            "payload": {
                "name": "products",
                "fields": [
                    {"name": "sku", "type": "text"},
                    {"name": "price", "type": "integer"}
                ]
            },
            "timestamp": 0
        }).to_string()))
        .unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    // 2. Verify schema.list_models returns the model
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({
            "request_id": "req-list",
            "op": "schema.list_models", 
            "resource": { "resource_type": "__models", "resource_id": null },
            "timestamp": 0
        }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let grant: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let cap_token = grant["token"].as_str().unwrap();

    let req = Request::builder()
        .uri("/v1/op/execute")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "execute_id": "exec-list", "token": cap_token, "timestamp": 0 }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    
    // Check if "products" is in the list (Result is the array directly)
    let models = json.as_array().expect("Expected models array");
    let product_model = models.iter().find(|m| m["name"] == "products").expect("Product model not found");
    let model_id = product_model["id"].as_str().expect("Model needs ID");

    // 3. Verify querying __fields to describe the model
    // Filter by model_id? Or just list all?
    // Opcode `resource.read` (which is List/Query) on `__fields` collection.
    // NOTE: Need to support filtering by model_id. 
    // State implementation supports constraints if passed.
    
    let req = Request::builder()
        .uri("/v1/op/request")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({
            "request_id": "req-fields",
            "op": "resource.read", 
            "resource": { "resource_type": "__fields", "resource_id": null },
            "input": { "model_id": model_id }, // Payload/Input as filter
            "timestamp": 0
        }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let grant: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let cap_token = grant["token"].as_str().unwrap();

    let req = Request::builder()
        .uri("/v1/op/execute")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ 
            "execute_id": "exec-fields", 
            "token": cap_token, 
            "payload": { "model_id": model_id }, // Pass filter in payload
            "timestamp": 0 
        }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let fields = json.as_array().expect("Expected fields array");
    
    // Verify we got fields and they belong to the model
    assert!(!fields.is_empty(), "Should have fields for created model");
    for f in fields {
        assert_eq!(f["model_id"], model_id);
    }
}

#[tokio::test]
async fn test_data_consistency_contracts() {
    let (app, code) = setup_app().await;
    let token = register_admin(app.clone(), &code).await;

    // 1. Create Model "items"
    // ... setup model ...
    let req = Request::builder().uri("/v1/op/request").method("POST").header("Content-Type", "application/json").header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({ "request_id": "r1", "op": "schema.create_model", "resource": { "resource_type": "__models", "resource_id": null }, "input": { "name": "items", "fields": [{"name":"cat","type":"text"}] }, "timestamp": 0 }).to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let grant: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()).unwrap();
    let cap = grant["token"].as_str().unwrap();
    
    let req = Request::builder().uri("/v1/op/execute").method("POST").header("Content-Type", "application/json")
        .body(Body::from(json!({ "execute_id": "e1", "token": cap, "payload": { "name": "items", "fields": [{"name":"cat","type":"text"}] }, "timestamp": 0 }).to_string())).unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    // 2. Insert Data (3 items: A, A, B)
    for cat in ["A", "A", "B"] {
        let req = Request::builder().uri("/v1/op/request").method("POST").header("Content-Type", "application/json").header("Authorization", format!("Bearer {}", token))
            .body(Body::from(json!({ "request_id": format!("req-create-{}", cat), "op": "resource.create", "resource": { "resource_type": "items", "resource_id": null }, "input": { "cat": cat }, "timestamp": 0 }).to_string())).unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let grant: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()).unwrap();
        let cap = grant["token"].as_str().unwrap();
        
        let req = Request::builder().uri("/v1/op/execute").method("POST").header("Content-Type", "application/json")
            .body(Body::from(json!({ "execute_id": format!("exec-create-{}", cat), "token": cap, "payload": { "cat": cat }, "timestamp": 0 }).to_string())).unwrap();
        let _ = app.clone().oneshot(req).await.unwrap();
    }
    
    // 2.5 Query ALL items to verify persistence
    let req = Request::builder().uri("/v1/op/request").method("POST").header("Content-Type", "application/json").header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({ "request_id": "req-query-all", "op": "resource.read", "resource": { "resource_type": "items", "resource_id": null }, "timestamp": 0 }).to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let grant: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()).unwrap();
    let cap = grant["token"].as_str().unwrap();

    let req = Request::builder().uri("/v1/op/execute").method("POST").header("Content-Type", "application/json")
        .body(Body::from(json!({ "execute_id": "exec-query-all", "token": cap, "timestamp": 0 }).to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 4096).await.unwrap()).unwrap();
    let all_items = json.as_array().expect("Expected items array");
    assert_eq!(all_items.len(), 3, "Should find 3 total items");

    // 3. Query with Filter (cat="A")
    let req = Request::builder().uri("/v1/op/request").method("POST").header("Content-Type", "application/json").header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({ "request_id": "req-query", "op": "resource.read", "resource": { "resource_type": "items", "resource_id": null }, "input": { "cat": "A" }, "timestamp": 0 }).to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let grant: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()).unwrap();
    let cap = grant["token"].as_str().unwrap();
    
    let req = Request::builder().uri("/v1/op/execute").method("POST").header("Content-Type", "application/json")
        .body(Body::from(json!({ "execute_id": "exec-query", "token": cap, "payload": { "cat": "A" }, "timestamp": 0 }).to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 4096).await.unwrap()).unwrap();
    let items = json.as_array().expect("Expected items array");
    assert_eq!(items.len(), 2, "Should find 2 items with cat=A");

    // 4. Count with Filter (cat="A")
    let req = Request::builder().uri("/v1/op/request").method("POST").header("Content-Type", "application/json").header("Authorization", format!("Bearer {}", token))
        .body(Body::from(json!({ "request_id": "req-count", "op": "resource.count", "resource": { "resource_type": "items", "resource_id": null }, "input": { "cat": "A" }, "timestamp": 0 }).to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let grant: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()).unwrap();
    let cap = grant["token"].as_str().unwrap();
    
    let req = Request::builder().uri("/v1/op/execute").method("POST").header("Content-Type", "application/json")
        .body(Body::from(json!({ "execute_id": "exec-count", "token": cap, "payload": { "cat": "A" }, "timestamp": 0 }).to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()).unwrap();
    let count = json["count"].as_u64().expect("Expected count");
    
    assert_eq!(count, 2, "Count should be 2 for cat=A");
}
