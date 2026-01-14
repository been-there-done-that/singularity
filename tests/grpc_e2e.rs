//! End-to-end tests for Transport Layer (gRPC).

use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tonic::transport::Server;
use tonic::Request;
use serde_json::json;

use singularity::capability::{CapabilitySigner, SigningKey};
use singularity::identity::{JwtVerifier, StandardClaims};
use singularity::policy::PolicyEngine;
use singularity::state::SqliteState;
use singularity::transport::AppState;
use singularity::transport::grpc::GrpcServer;
use singularity::transport::grpc::pb::singularity_client::SingularityClient;
use singularity::transport::grpc::pb::{OpRequestProto, ResourceProto, OpExecuteProto};

// Helper to convert JSON -> Prost Struct manually for test client
fn json_to_struct(v: serde_json::Value) -> prost_types::Struct {
    // We can use the helper from the server logic if we expose it, or rewrite it briefly.
    // For test, we can just use simple logic or even just empty/basic.
    // Ideally we reuse the logic. But it's private in server.rs.
    // I will rewrite a simple recursive one here.
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
    let jwt_secret = b"grpc-test-secret";
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://grpc.test",
        "singularity-grpc",
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
    );

    // 2. Start gRPC Server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let grpc_service = GrpcServer::new(app_state);
    
    // We need to adapt our GrpcServer (which implements Singularity) to tonic service.
    // Imports:
    use singularity::transport::grpc::pb::singularity_server::SingularityServer;
    
    let server_future = Server::builder()
        .add_service(SingularityServer::new(grpc_service))
        .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener));

    tokio::spawn(server_future);

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 3. Connect Client
    // tonic::transport::Channel::from_shared(...).connect()...
    let uri = format!("http://{}", addr);
    let mut client = SingularityClient::connect(uri).await.unwrap();

    // 4. Prepare Identity Token
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    
    let claims = StandardClaims {
        sub: "user-grpc".to_string(),
        roles: vec!["admin".to_string()],
        groups: vec![],
        email: None,
        name: None,
        iat: Some(now),
        exp: Some(now + 3600),
        nbf: Some(now),
        iss: Some("https://grpc.test".to_string()),
        aud: Some(serde_json::json!("singularity-grpc")),
    };
    
    let jwt = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(jwt_secret)
    ).unwrap();


    // 5. Call Request
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
