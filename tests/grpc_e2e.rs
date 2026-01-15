//! End-to-end tests for Transport Layer (gRPC).
//!
//! Tests the gRPC transport with real session authentication.

use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tonic::transport::Server;
use tonic::Request;
use serde_json::json;

use singularity::capability::{CapabilitySigner, SigningKey};
use singularity::identity::JwtVerifier;
use singularity::policy::PolicyEngine;
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::grpc::GrpcServer;
use singularity::transport::grpc::pb::singularity_client::SingularityClient;
use singularity::transport::grpc::pb::{OpRequestProto, ResourceProto, OpExecuteProto};
use singularity::transport::http::app;
use axum::{body::Body, http::Request as AxumRequest, http::StatusCode};

// Helper to convert JSON -> Prost Struct
fn json_to_struct(v: serde_json::Value) -> prost_types::Struct {
    from_json(v)
}

fn from_json(v: serde_json::Value) -> prost_types::Struct {
    match v {
        serde_json::Value::Object(m) => {
             prost_types::Struct {
                fields: m.into_iter().map(|(k, v)| (k, json_val_to_proto(v))).collect(),
            }
        },
        _ => prost_types::Struct::default(),
    }
}

fn json_val_to_proto(v: serde_json::Value) -> prost_types::Value {
    use prost_types::value::Kind;
    let kind = match v {
        serde_json::Value::Null => Some(Kind::NullValue(0)),
        serde_json::Value::Bool(b) => Some(Kind::BoolValue(b)),
        serde_json::Value::Number(n) => Some(Kind::NumberValue(n.as_f64().unwrap_or(0.0))),
        serde_json::Value::String(s) => Some(Kind::StringValue(s)),
        serde_json::Value::Array(l) => Some(Kind::ListValue(prost_types::ListValue {
            values: l.into_iter().map(json_val_to_proto).collect(),
        })),
        serde_json::Value::Object(m) => Some(Kind::StructValue(from_json(serde_json::Value::Object(m)))),
    };
    prost_types::Value { kind }
}


#[tokio::test]
async fn test_grpc_transport_flow() {
    // 1. Setup Components
    let jwt_secret = b"grpc-test-secret-key-32-chars!";
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://singularity.local",
        "singularity",
        jwt_secret.to_vec(),
    ));

    let policy = PolicyEngine::new();
    let sys_policy = "true".to_string(); // Allow all
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
        sys_policy,
        jwt_secret.to_vec(),
    );

    // 2. Get a real JWT via HTTP register (need to start HTTP for this)
    let http_app = app(app_state.clone());
    
    let register_body = json!({
        "username": "grpcuser",
        "password": "testpassword123",
        "device_name": "grpc-test"
    });

    let register_req = AxumRequest::builder()
        .uri("/auth/register")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let register_resp = tower::util::ServiceExt::oneshot(http_app, register_req).await.unwrap();
    assert_eq!(register_resp.status(), StatusCode::OK);

    let register_bytes = axum::body::to_bytes(register_resp.into_body(), 2048).await.unwrap();
    let auth_response: serde_json::Value = serde_json::from_slice(&register_bytes).unwrap();
    let jwt = auth_response["token"].as_str().unwrap().to_string();

    // 3. Start gRPC Server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let grpc_service = GrpcServer::new(app_state);
    
    use singularity::transport::grpc::pb::singularity_server::SingularityServer;
    
    let server_future = Server::builder()
        .add_service(SingularityServer::new(grpc_service))
        .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener));

    tokio::spawn(server_future);

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 4. Connect Client
    let uri = format!("http://{}", addr);
    let mut client = SingularityClient::connect(uri).await.unwrap();

    // 5. Call Request with real JWT
    let req = OpRequestProto {
        request_id: "req-grpc-1".to_string(),
        op: "resource.create".to_string(),
        resource: Some(ResourceProto {
            r#type: "doc".to_string(),
            id: Some("grpc-1".to_string()),
        }),
        input: Some(json_to_struct(json!({"title": "gRPC Test"}))),
    };

    let mut tonic_req = Request::new(req);
    tonic_req.metadata_mut().insert("authorization", format!("Bearer {}", jwt).parse().unwrap());

    let response = client.request(tonic_req).await.unwrap();
    let grant = response.into_inner();

    assert_eq!(grant.request_id, "req-grpc-1");
    assert!(!grant.token.is_empty());

    // 6. Call Execute
    let exec = OpExecuteProto {
        execute_id: "exec-grpc-1".to_string(),
        token: grant.token,
        payload: Some(json_to_struct(json!({"title": "gRPC Test", "content": "Via Tonic"}))),
    };

    let exec_resp = client.execute(Request::new(exec)).await.unwrap();
    let result = exec_resp.into_inner();

    // Result is oneof
    match result.result {
        Some(singularity::transport::grpc::pb::execution_result_proto::Result::RowsAffected(n)) => {
            assert_eq!(n, 1);
        },
        _ => panic!("Expected RowsAffected"),
    }
}
