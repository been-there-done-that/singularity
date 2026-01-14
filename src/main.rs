use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, warn};

use singularity::state::SqliteState;
use singularity::migration::manager::MigrationManager;
use singularity::transport::{AppState, http, grpc};
use singularity::identity::JwtVerifier;
use singularity::policy::PolicyEngine;
use singularity::capability::{CapabilitySigner, SigningKey};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    if env::var("RUST_LOG").is_err() {
        unsafe { env::set_var("RUST_LOG", "info"); }
    }
    tracing_subscriber::fmt::init();

    info!("Starting Singularity Kernel...");

    // 1. Configuration
    let db_path = env::var("DATABASE_URL").unwrap_or_else(|_| "singularity.db".to_string());
    let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
        warn!("JWT_SECRET not set. Using insecure default for development.");
        "dev-secret-do-not-use".to_string()
    });
    let http_port = env::var("PORT").unwrap_or_else(|_| "3000".to_string()).parse::<u16>()?;
    let grpc_port = env::var("GRPC_PORT").unwrap_or_else(|_| "50051".to_string()).parse::<u16>()?;

    // 2. Initialize State (Persisted)
    info!("Opening database at: {}", db_path);
    let mut state = SqliteState::open(&db_path)?;

    // 3. Run Internal Migrations
    info!("Running internal migrations...");
    let migration_manager = MigrationManager::new();
    migration_manager.run(&mut state)?;
    info!("Internal migrations applied successfully.");

    // 4. Initialize Kernel Components
    let identity = Arc::new(JwtVerifier::with_hmac_secret(
        "https://singularity.local", // Issuer
        "singularity",               // Audience
        jwt_secret.into_bytes(),
    ));

    let policy = PolicyEngine::new();
    
    // Load system policy
    let sys_policy = std::fs::read_to_string("src/policy/defaults.rhai")
        .unwrap_or_else(|_| {
            warn!("defaults.rhai not found, using secure default (deny all)");
            "false".to_string()
        });

    // Signing Key (Ephemeral for now - resets on restart)
    // Production should load this from secure storage/env
    warn!("Using ephemeral signing key. Tokens will be invalid after restart.");
    let signer = CapabilitySigner::new(SigningKey::generate());

    let app_state = AppState::new(identity, policy, signer, state, sys_policy);

    // 5. Start Transports
    let http_addr = SocketAddr::from(([0, 0, 0, 0], http_port));
    let grpc_addr = SocketAddr::from(([0, 0, 0, 0], grpc_port));

    // HTTP Server
    let http_state = app_state.clone();
    let http_router = http::app(http_state);
    
    let http_server = async move {
        info!("HTTP server listening on {}", http_addr);
        let listener = TcpListener::bind(http_addr).await.unwrap();
        axum::serve(listener, http_router).await
    };

    // gRPC Server
    let grpc_service = grpc::GrpcServer::new(app_state);
    let grpc_server = async move {
        info!("gRPC server listening on {}", grpc_addr);
        tonic::transport::Server::builder()
            .add_service(singularity::transport::grpc::pb::singularity_server::SingularityServer::new(grpc_service))
            .serve(grpc_addr)
            .await
    };

    // Run both
    tokio::select! {
        _ = http_server => { info!("HTTP server stopped"); },
        _ = grpc_server => { info!("gRPC server stopped"); },
        _ = tokio::signal::ctrl_c() => { info!("Received Ctrl-C, shutting down"); },
    }

    Ok(())
}
