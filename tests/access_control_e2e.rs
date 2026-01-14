use std::sync::Arc;
use serde_json::json;
use singularity::transport::{AppState, pipeline};
use singularity::protocol::{OpRequest, OpExecute, Resource, FieldSet};
use singularity::policy::{PolicyEngine, PolicySubject};
use singularity::identity::{IdentityVerifier};
use singularity::state::SqliteState;
use singularity::capability::{SigningKey, CapabilitySigner};
use singularity::migration::manager::MigrationManager;

// Mock Identity Verifier that trusts everything
struct MockIdentity(String, Vec<String>); // ID, Roles
impl IdentityVerifier for MockIdentity {
    fn verify(&self, _token: &str, _now: u64) -> Result<PolicySubject, singularity::identity::IdentityError> {
        Ok(PolicySubject {
            id: self.0.clone(),
            roles: self.1.clone(),
            claims: Default::default(),
            internal_id: None, // internal_id will be enriched by pipeline
        })
    }
}

fn create_shared_state() -> Arc<SqliteState> {
    let mut state = SqliteState::in_memory().unwrap();
    let migration_manager = MigrationManager::new();
    migration_manager.run(&mut state).unwrap();
    Arc::new(state)
}

fn create_app(state: Arc<SqliteState>, user_id: &str, roles: Vec<&str>) -> AppState {
    let identity = Arc::new(MockIdentity(user_id.to_string(), roles.iter().map(|s| s.to_string()).collect()));
    let sys_policy = std::fs::read_to_string("src/policy/defaults.rhai").unwrap();
    let signing_key = SigningKey::generate();

    AppState {
        state,
        identity,
        policy: Arc::new(PolicyEngine::new()),
        system_policy: sys_policy,
        signer: Arc::new(CapabilitySigner::new(signing_key)),
    }
}

#[test]
fn test_provisioning_and_ownership() {
    let state = create_shared_state();
    let now = 1704067200;

    let admin_app = create_app(state.clone(), "user-admin", vec!["admin"]);
    let app = create_app(state.clone(), "user-alice", vec!["user"]);
    let other_app = create_app(state.clone(), "user-bob", vec!["user"]);

    // 1. Admin creates "todo" model
    let create_model_req = OpRequest {
        request_id: "req-0".into(),
        op: "schema.create_model".into(),
        resource: Resource::instance("__models", "todo"),
        fields: None,
        input: Some(json!({"name": "todo"})),
        timestamp: now,
    };
    
    let grant = pipeline::process_request(&admin_app, "mock-token", create_model_req, now).expect("Admin should be allowed");
    let exec = OpExecute {
        execute_id: "exec-0".into(),
        token: grant.token,
        payload: Some(json!({"name": "todo"})),
        timestamp: now,
    };
    pipeline::process_execute(&admin_app, exec, now).expect("Execution failed");

    // 1b. Admin adds "title" field
    let add_field_req = OpRequest {
        request_id: "req-0b".into(),
        op: "schema.add_field".into(),
        resource: Resource::instance("__fields", "field-title"),
        fields: None,
        input: Some(json!({
            "model_id": "todo",
            "name": "title",
            "field_type": { "type": "String" }, // Assuming type structure
            "required": true
        })),
        timestamp: now,
    };
    let grant = pipeline::process_request(&admin_app, "mock-token", add_field_req, now).expect("Admin add field allowed");
    let exec = OpExecute {
        execute_id: "exec-0b".into(),
        token: grant.token,
        payload: Some(json!({
            "model_id": "todo",
            "name": "title",
            "field_type": { "type": "String" },
            "required": true
        })),
        timestamp: now,
    };
    pipeline::process_execute(&admin_app, exec, now).expect("Add field failed");

    // 2. User Creates Todo
    let req = OpRequest {
        request_id: "req-1".into(),
        op: "resource.create".into(),
        resource: Resource::instance("todo", "todo-1"), 
        fields: Some(FieldSet::all()),
        input: Some(json!({"title": "Buy milk"})),
        timestamp: now,
    };
    let grant = pipeline::process_request(&app, "mock-token", req.clone(), now).expect("User should be allowed to create");
    
    let exec = OpExecute {
        execute_id: "exec-1".into(),
        token: grant.token,
        payload: Some(json!({"title": "Buy milk"})),
        timestamp: now,
    };
    
    pipeline::process_execute(&app, exec, now).expect("Create failed");

    // 3. User Reads Todo (Should be allowed as owner)
    let read_req = OpRequest {
        request_id: "req-2".into(),
        op: "resource.read".into(),
        resource: Resource::instance("todo", "todo-1"),
        fields: Some(FieldSet::all()),
        input: None,
        timestamp: now,
    };
    
    let grant = pipeline::process_request(&app, "mock-token", read_req.clone(), now).expect("Owner should read");
    let exec = OpExecute {
        execute_id: "exec-2".into(),
        token: grant.token,
        payload: None,
        timestamp: now,
    };
    let res = pipeline::process_execute(&app, exec, now).expect("Read failed");
    if let singularity::execution::ExecutionResult::Read { data } = res {
        assert_eq!(data["title"], "Buy milk");
        assert!(data.get("owner_id").is_some());
    } else {
        panic!("Expected read result");
    }

    // 4. Other User Reads Todo (Should deny)
    let deny_req = OpRequest {
        request_id: "req-3".into(),
        op: "resource.read".into(),
        resource: Resource::instance("todo", "todo-1"), // Alice's todo
        fields: Some(FieldSet::all()),
        input: None,
        timestamp: now,
    };
    
    let err = pipeline::process_request(&other_app, "mock-token", deny_req, now);
    assert!(err.is_err(), "Other user should be denied access to Alice's data");
}
