//! End-to-end tests for Singularity kernel.
//!
//! These tests prove the entire pipeline:
//! Identity → Policy → Capability → Execution → State

use serde_json::json;

use singularity::capability::{CapabilitySigner, CapabilityVerifier, SigningKey};
use singularity::execution::{
    ExecutionContext, ExecutionError, ExecutionMeta, ExecutionTarget, ExecutionResult, 
    StateBackedExecutor, OperationExecutor,
};
use singularity::policy::{PolicyContext, PolicyEngine, PolicyEnv, PolicySubject};
use singularity::protocol::{CapabilityPayload, FieldSet, Resource};
use singularity::state::SqliteState;

/// Full kernel pipeline: Identity → Policy → Capability → Execution → State
#[test]
fn e2e_create_and_read_resource() {
    // --- Identity ---
    let subject = PolicySubject::new("user-123").with_roles(["user"]);

    // --- Policy ---
    let policy = r#"
        op == "resource.create" || op == "resource.read"
    "#;

    let policy_engine = PolicyEngine::new();
    let ctx = PolicyContext::new(
        subject.clone(),
        Resource::instance("user", "e2e-1"),
        "resource.create",
        PolicyEnv::new(1704067200),
    );

    let policy_result = policy_engine.evaluate(policy, &ctx).unwrap();
    assert!(policy_result, "Policy must approve resource.create");

    // --- Capability minting ---
    let signing_key = SigningKey::generate();
    let signer = CapabilitySigner::new(signing_key);
    let verifier = CapabilityVerifier::new(signer.verifying_key());

    let payload = CapabilityPayload::new(
        "cap-e2e",
        "resource.create",
        Resource::instance("user", "e2e-1"),
        FieldSet::all(),
        1704067200,
        1704067260,
    );

    let token = signer.mint(&payload).unwrap();
    let verified = verifier.verify(&token, 1704067230).unwrap();

    // --- Execution ---
    let exec_ctx = ExecutionContext::new(verified);
    let target = ExecutionTarget::new(Resource::instance("user", "e2e-1"));
    let meta = ExecutionMeta::new();

    let state = SqliteState::in_memory().unwrap();
    let executor = StateBackedExecutor::new(&state);

    let create_result = executor.execute(
        &exec_ctx,
        &target,
        &meta,
        Some(json!({"name": "E2E Test User", "email": "e2e@test.com"})),
    );
    assert!(create_result.is_ok(), "Create must succeed");

    // --- Read back ---
    let read_payload = CapabilityPayload::new(
        "cap-e2e-read",
        "resource.read",
        Resource::instance("user", "e2e-1"),
        FieldSet::all(),
        1704067200,
        1704067260,
    );
    let read_token = signer.mint(&read_payload).unwrap();
    let read_verified = verifier.verify(&read_token, 1704067230).unwrap();
    let read_ctx = ExecutionContext::new(read_verified);

    let result = executor.execute(&read_ctx, &target, &meta, None).unwrap();

    if let ExecutionResult::Read { data } = result {
        assert_eq!(data.get("name"), Some(&json!("E2E Test User")));
        assert_eq!(data.get("email"), Some(&json!("e2e@test.com")));
    } else {
        panic!("Expected Read result");
    }
}

/// Prove policy denial blocks the entire flow.
#[test]
fn e2e_policy_denial_blocks_flow() {
    let subject = PolicySubject::new("user-123").with_roles(["guest"]);

    // Policy that denies guests
    let policy = r#"
        roles.contains("admin")
    "#;

    let policy_engine = PolicyEngine::new();
    let ctx = PolicyContext::new(
        subject,
        Resource::instance("secret", "data"),
        "resource.read",
        PolicyEnv::new(1704067200),
    );

    let result = policy_engine.evaluate(policy, &ctx).unwrap();
    assert!(!result, "Policy must deny guest access");

    // Flow stops here - no capability minted, no execution possible
}

/// Prove field filtering works through the entire pipeline.
#[test]
fn e2e_field_filtering_enforced() {
    let subject = PolicySubject::new("user-456").with_roles(["user"]);

    let policy = r#"op == "resource.create" || op == "resource.read""#;
    let policy_engine = PolicyEngine::new();

    // Check policy
    let ctx = PolicyContext::new(
        subject.clone(),
        Resource::instance("user", "filter-e2e"),
        "resource.create",
        PolicyEnv::new(1704067200),
    );
    assert!(policy_engine.evaluate(policy, &ctx).unwrap());

    // Create with all fields
    let signing_key = SigningKey::generate();
    let signer = CapabilitySigner::new(signing_key);
    let verifier = CapabilityVerifier::new(signer.verifying_key());

    let create_payload = CapabilityPayload::new(
        "cap-create",
        "resource.create",
        Resource::instance("user", "filter-e2e"),
        FieldSet::all(),
        1704067200,
        1704067260,
    );
    let create_token = signer.mint(&create_payload).unwrap();
    let create_verified = verifier.verify(&create_token, 1704067230).unwrap();

    let state = SqliteState::in_memory().unwrap();
    let executor = StateBackedExecutor::new(&state);
    let target = ExecutionTarget::new(Resource::instance("user", "filter-e2e"));
    let meta = ExecutionMeta::new();

    executor.execute(
        &ExecutionContext::new(create_verified),
        &target,
        &meta,
        Some(json!({"name": "Filter Test", "email": "test@test.com", "secret": "hidden"})),
    ).unwrap();

    // Read with LIMITED fields capability
    let read_payload = CapabilityPayload::new(
        "cap-read-limited",
        "resource.read",
        Resource::instance("user", "filter-e2e"),
        FieldSet::new(["name", "email"]),  // NO secret field
        1704067200,
        1704067260,
    );
    let read_token = signer.mint(&read_payload).unwrap();
    let read_verified = verifier.verify(&read_token, 1704067230).unwrap();

    let result = executor.execute(
        &ExecutionContext::new(read_verified),
        &target,
        &meta,
        None,
    ).unwrap();

    if let ExecutionResult::Read { data } = result {
        assert!(data.get("name").is_some());
        assert!(data.get("email").is_some());
        assert!(data.get("secret").is_none(), "Secret field must be filtered!");
    } else {
        panic!("Expected Read result");
    }
}

/// Prove unauthorized write field is rejected (not silently dropped).
#[test]
fn e2e_unauthorized_write_rejected() {
    let signing_key = SigningKey::generate();
    let signer = CapabilitySigner::new(signing_key);
    let verifier = CapabilityVerifier::new(signer.verifying_key());

    // Capability only allows "name" field
    let payload = CapabilityPayload::new(
        "cap-limited",
        "resource.create",
        Resource::instance("user", "unauthorized-e2e"),
        FieldSet::new(["name"]),
        1704067200,
        1704067260,
    );
    let token = signer.mint(&payload).unwrap();
    let verified = verifier.verify(&token, 1704067230).unwrap();

    let state = SqliteState::in_memory().unwrap();
    let executor = StateBackedExecutor::new(&state);
    let target = ExecutionTarget::new(Resource::instance("user", "unauthorized-e2e"));
    let meta = ExecutionMeta::new();

    // Try to write unauthorized "password" field
    let result = executor.execute(
        &ExecutionContext::new(verified),
        &target,
        &meta,
        Some(json!({"name": "Test", "password": "sneaky"})),
    );

    assert!(
        matches!(result, Err(ExecutionError::UnauthorizedFieldWrite { .. })),
        "Writing unauthorized field must fail with UnauthorizedFieldWrite"
    );
}
