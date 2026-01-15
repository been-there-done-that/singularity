use singularity::capability::{CapabilitySigner, CapabilityVerifier, SigningKey};
use singularity::execution::{ExecutionContext, ExecutionMeta, ExecutionTarget, StateBackedExecutor, OperationExecutor};
use singularity::migration::manager::MigrationManager;
use singularity::protocol::{CapabilityPayload, FieldSet, Resource, opcode::*};
use singularity::state::{SqliteState, State};
use serde_json::json;

fn setup() -> (SqliteState, SigningKey) {
    let mut state = SqliteState::in_memory().unwrap();
    let manager = MigrationManager::new();
    manager.run(&mut state).unwrap();
    (state, SigningKey::generate())
}

fn create_context(op: &str, resource: Resource) -> ExecutionContext {
    create_context_with_user(op, resource, "test-user-uuid")
}

fn create_context_with_user(op: &str, resource: Resource, user_id: &str) -> ExecutionContext {
    let signing_key = SigningKey::generate();
    let signer = CapabilitySigner::new(signing_key);
    let verifier = CapabilityVerifier::new(signer.verifying_key());
    let payload = CapabilityPayload::new("test", op, resource, FieldSet::all(), 0, 60)
        .with_internal_user_id(user_id);
    let token = signer.mint(&payload).unwrap();
    let verified = verifier.verify(&token, 30).unwrap();
    ExecutionContext::new(verified)
}

#[test]
fn test_schema_persistence_flow() {
    let (state, _) = setup();
    let executor = StateBackedExecutor::new(&state);
    let meta = ExecutionMeta::new();

    // 1. Create Model "test_model"
    let model_id = "m_test";
    let target_res = Resource::instance("__models", model_id);
    let ctx = create_context(SCHEMA_CREATE_MODEL, target_res.clone());
    let target = ExecutionTarget::new(target_res);
    
    let model_payload = json!({
        "name": "test_model",
        "namespace": "public",
        "created_at": 1234567890
    });

    executor.execute(&ctx, &target, &meta, Some(model_payload)).expect("create model failed");

    // 2. Add Field "title"
    let field_id = "f_title";
    let field_res = Resource::instance("__fields", field_id);
    let ctx_field = create_context(SCHEMA_ADD_FIELD, field_res.clone());
    let target_field = ExecutionTarget::new(field_res);

    let field_payload = json!({
        "model_id": model_id,
        "name": "title",
        "field_type": { "type": "String", "config": null },
        "required": true,
        "unique": false,
        "default": "Untitled",
        "created_at": 1234567891
    });

    executor.execute(&ctx_field, &target_field, &meta, Some(field_payload)).expect("add field failed");

    // 3. Read Model back (should include fields)
    let ctx_read = create_context(RESOURCE_READ, Resource::instance("__models", model_id));
    let target_read = ExecutionTarget::new(Resource::instance("__models", model_id));
    
    let result = executor.execute(&ctx_read, &target_read, &meta, None).expect("read model failed");
    
    if let singularity::execution::ExecutionResult::Read { data } = result {
        assert_eq!(data["name"], "test_model");
        let fields = data["fields"].as_array().expect("fields should be array");
        assert_eq!(fields.len(), 5);
        let title_field = fields.iter().find(|f| f["name"] == "title").expect("title field should exist");
        assert_eq!(title_field["required"], true);
    } else {
        panic!("expected read result");
    }

    // 4. Drop Field "title"
    // Target is the field instance
    let drop_res = Resource::instance("__fields", field_id);
    let ctx_drop = create_context(SCHEMA_DROP_FIELD, drop_res.clone());
    let target_drop = ExecutionTarget::new(drop_res);

    executor.execute(&ctx_drop, &target_drop, &meta, None).expect("drop field failed");

    // 5. Read Model back (should have 0 fields)
    let result_after = executor.execute(&ctx_read, &target_read, &meta, None).expect("read model failed");

    if let singularity::execution::ExecutionResult::Read { data } = result_after {
        assert_eq!(data["name"], "test_model");
        let fields = data["fields"].as_array().expect("fields should be array");
        assert_eq!(fields.len(), 4);
    }

    // 6. Verify Physical Table Storage (STRICT mode)
    // Create a new model "users" with typed fields
    let user_model_id = "m_users";
    let user_model_payload = json!({
        "name": "users",
        "namespace": "public",
        "created_at": 1234567895
    });
    let ctx_model_users = create_context(SCHEMA_CREATE_MODEL, Resource::instance("__models", user_model_id));
    executor.execute(&ctx_model_users, &ExecutionTarget::new(Resource::instance("__models", user_model_id)), &meta, Some(user_model_payload)).expect("create users model failed");
    
    // Add "age" field (Integer)
    let age_field_payload = json!({
        "model_id": user_model_id,
        "name": "age",
        "field_type": { "type": "Int", "config": null },
        "required": false,
        "unique": false,
        "default": null,
        "created_at": 1234567896
    });
    let ctx_field_age = create_context(SCHEMA_ADD_FIELD, Resource::instance("__fields", "f_age"));
    executor.execute(&ctx_field_age, &ExecutionTarget::new(Resource::instance("__fields", "f_age")), &meta, Some(age_field_payload)).expect("add age field failed");

    // Write data to "users" physical table
    let user_id = "curr_user";
    let user_payload = json!({
        "age": 30
    });
    let ctx_write_user = create_context(RESOURCE_CREATE, Resource::instance("users", user_id));
    executor.execute(&ctx_write_user, &ExecutionTarget::new(Resource::instance("users", user_id)), &meta, Some(user_payload)).expect("write user failed");

    // Read back user
    let ctx_read_user = create_context(RESOURCE_READ, Resource::instance("users", user_id));
    let read_result = executor.execute(&ctx_read_user, &ExecutionTarget::new(Resource::instance("users", user_id)), &meta, None).expect("read user failed");
    
    if let singularity::execution::ExecutionResult::Read { data } = read_result {
        assert_eq!(data["age"], 30);
    } else {
        panic!("expected read result for user");
    }

    // Verify STRICT enforcement: Try writing String to Int field
    let invalid_payload = json!({
        "age": "thirty" // Should fail or error in STRICT mode (or rusqlite conversion)
    });
    // With our current logic, "thirty" is passed as string to INTEGER column. STRICT table should reject.
    let ctx_write_invalid = create_context(RESOURCE_CREATE, Resource::instance("users", "invalid_user"));
    let err = executor.execute(&ctx_write_invalid, &ExecutionTarget::new(Resource::instance("users", "invalid_user")), &meta, Some(invalid_payload));
    
    assert!(err.is_err(), "Should fail to write string to integer column in STRICT table");
}

#[test]
fn test_schema_introspection_access_control() {
    let (state, _) = setup();
    let executor = StateBackedExecutor::new(&state);
    let meta = ExecutionMeta::new();

    let user_a = "user-a-uuid";
    let user_b = "user-b-uuid";

    // 1. User A creates "model_a"
    let model_id = "m_model_a";
    let target_res = Resource::instance("__models", model_id);
    let ctx_a_create = create_context_with_user(SCHEMA_CREATE_MODEL, target_res.clone(), user_a);
    let target = ExecutionTarget::new(target_res);
    
    let model_payload = json!({
        "name": "model_a",
        "namespace": "public",
        "created_at": 100
    });

    executor.execute(&ctx_a_create, &target, &meta, Some(model_payload)).expect("User A create model failed");

    // 2. User A lists models -> Should see "model_a"
    let list_res = Resource::collection("__models");
    let ctx_a_list = create_context_with_user(SCHEMA_LIST_MODELS, list_res.clone(), user_a);
    let target_list = ExecutionTarget::new(list_res);

    let result_a = executor.execute(&ctx_a_list, &target_list, &meta, None).expect("User A list failed");
    
    if let singularity::execution::ExecutionResult::Read { data } = result_a {
        let list = data.as_array().expect("result should be array");
        assert!(list.iter().any(|m| m["id"] == model_id), "User A should see model_a");
        assert!(list.iter().any(|m| m["owner_id"] == user_a), "Model should have owner_id");
    } else {
        panic!("expected read result");
    }

    // 3. User B lists models -> Should NOT see "model_a" (Empty list or filtered)
    let ctx_b_list = create_context_with_user(SCHEMA_LIST_MODELS, Resource::collection("__models"), user_b);
    let result_b = executor.execute(&ctx_b_list, &target_list, &meta, None).expect("User B list failed");

    if let singularity::execution::ExecutionResult::Read { data } = result_b {
        let list = data.as_array().expect("result should be array");
        assert!(!list.iter().any(|m| m["id"] == model_id), "User B should NOT see model_a");
        assert_eq!(list.len(), 0, "User B owns nothing, should see nothing");
    } else {
        panic!("expected read result");
    }

    // 4. User B tries to READ "model_a" directly -> Should be NOT FOUND (due to RLS)
    let ctx_b_read = create_context_with_user(RESOURCE_READ, Resource::instance("__models", model_id), user_b);
    let target_read = ExecutionTarget::new(Resource::instance("__models", model_id));

    let result_b_read = executor.execute(&ctx_b_read, &target_read, &meta, None);
    
    match result_b_read {
        Err(singularity::execution::ExecutionError::ResourceNotFound { .. }) => {
            // Success: Not Found (masked)
        },
        Ok(_) => panic!("User B should not be able to read model_a"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    // 5. User A reads "model_a" -> Success
    let ctx_a_read = create_context_with_user(RESOURCE_READ, Resource::instance("__models", model_id), user_a);
    let result_a_read = executor.execute(&ctx_a_read, &target_read, &meta, None);
    assert!(result_a_read.is_ok(), "User A should read model_a");
}

#[test]
fn test_schema_introspection_enrichment() {
    let (state, _) = setup();
    let executor = StateBackedExecutor::new(&state);
    let meta = ExecutionMeta::new();
    let user_a = "user-a-enrich";

    // 1. Create Model
    let model_id = "m_enrich";
    let ctx = create_context_with_user(SCHEMA_CREATE_MODEL, Resource::instance("__models", model_id), user_a);
    let target = ExecutionTarget::new(Resource::instance("__models", model_id));
    
    // Payload uses implicit owner injection (Source of Truth hardening verified implicitly)
    let payload = json!({
        "name": "enrich_model",
        "namespace": "public",
        "created_at": 200
    });
    executor.execute(&ctx, &target, &meta, Some(payload)).expect("create model failed");

    // 2. List Models (Introspection)
    // op must be "schema.list_models" to trigger enrichment logic
    let ctx_list = create_context_with_user(SCHEMA_LIST_MODELS, Resource::collection("__models"), user_a);
    let target_list = ExecutionTarget::new(Resource::collection("__models"));

    let result = executor.execute(&ctx_list, &target_list, &meta, None).expect("list failed");

    if let singularity::execution::ExecutionResult::Read { data } = result {
        let list = data.as_array().expect("result array");
        let model = list.iter().find(|m| m["id"] == model_id).expect("model found");
        
        // 3. Verify Ownership Metadata
        let ownership = model.get("ownership").expect("ownership metadata present");
        assert_eq!(ownership["column"], "owner_id");
        assert_eq!(ownership["principal"], "__internal_users");
    } else {
        panic!("expected read result");
    }
}
