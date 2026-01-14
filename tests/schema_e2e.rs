use singularity::capability::{CapabilitySigner, CapabilityVerifier, SigningKey};
use singularity::execution::{ExecutionContext, ExecutionMeta, ExecutionTarget, StateBackedExecutor, OperationExecutor};
use singularity::migration::manager::MigrationManager;
use singularity::protocol::{CapabilityPayload, FieldSet, Resource};
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
    let create_op = "schema.create_model";
    let target_res = Resource::instance("__models", model_id);
    let ctx = create_context(create_op, target_res.clone());
    let target = ExecutionTarget::new(target_res);
    
    let model_payload = json!({
        "name": "test_model",
        "namespace": "public",
        "created_at": 1234567890
    });

    executor.execute(&ctx, &target, &meta, Some(model_payload)).expect("create model failed");

    // 2. Add Field "title"
    let field_id = "f_title";
    let add_op = "schema.add_field";
    let field_res = Resource::instance("__fields", field_id);
    let ctx_field = create_context(add_op, field_res.clone());
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
    let read_op = "resource.read"; // Internal read uses generic read? 
    // Wait, StateBackedExecutor maps "resource.read" to state.read.
    // User might need "schema.read_model" but currently generic read works if policy allows "resource.read" on "__models".
    // Or we use "resource.read" with target "__models".
    // Our executor allows "resource.read".
    let ctx_read = create_context(read_op, Resource::instance("__models", model_id));
    let target_read = ExecutionTarget::new(Resource::instance("__models", model_id));
    
    let result = executor.execute(&ctx_read, &target_read, &meta, None).expect("read model failed");
    
    if let singularity::execution::ExecutionResult::Read { data } = result {
        assert_eq!(data["name"], "test_model");
        let fields = data["fields"].as_array().expect("fields should be array");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["name"], "title");
        assert_eq!(fields[0]["required"], true);
    } else {
        panic!("expected read result");
    }

    // 4. Drop Field "title"
    let drop_op = "schema.drop_field";
    // Target is the field instance
    let drop_res = Resource::instance("__fields", field_id);
    let ctx_drop = create_context(drop_op, drop_res.clone());
    let target_drop = ExecutionTarget::new(drop_res);

    executor.execute(&ctx_drop, &target_drop, &meta, None).expect("drop field failed");

    // 5. Read Model back (should have 0 fields)
    let result_after = executor.execute(&ctx_read, &target_read, &meta, None).expect("read model failed");

    if let singularity::execution::ExecutionResult::Read { data } = result_after {
        assert_eq!(data["name"], "test_model");
        let fields = data["fields"].as_array().expect("fields should be array");
        assert_eq!(fields.len(), 0);
    }

    // 6. Verify Physical Table Storage (STRICT mode)
    // Create a new model "users" with typed fields
    let user_model_id = "m_users";
    let user_model_payload = json!({
        "name": "users",
        "namespace": "public",
        "created_at": 1234567895
    });
    let ctx_model_users = create_context("schema.create_model", Resource::instance("__models", user_model_id));
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
    let ctx_field_age = create_context("schema.add_field", Resource::instance("__fields", "f_age"));
    executor.execute(&ctx_field_age, &ExecutionTarget::new(Resource::instance("__fields", "f_age")), &meta, Some(age_field_payload)).expect("add age field failed");

    // Write data to "users" physical table
    let user_id = "curr_user";
    let user_payload = json!({
        "age": 30
    });
    let ctx_write_user = create_context("resource.create", Resource::instance("users", user_id));
    executor.execute(&ctx_write_user, &ExecutionTarget::new(Resource::instance("users", user_id)), &meta, Some(user_payload)).expect("write user failed");

    // Read back user
    let ctx_read_user = create_context("resource.read", Resource::instance("users", user_id));
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
    let ctx_write_invalid = create_context("resource.create", Resource::instance("users", "invalid_user"));
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
    let create_op = "schema.create_model";
    let target_res = Resource::instance("__models", model_id);
    let ctx_a_create = create_context_with_user(create_op, target_res.clone(), user_a);
    let target = ExecutionTarget::new(target_res);
    
    let model_payload = json!({
        "name": "model_a",
        "namespace": "public",
        "created_at": 100
    });

    executor.execute(&ctx_a_create, &target, &meta, Some(model_payload)).expect("User A create model failed");

    // 2. User A lists models -> Should see "model_a"
    let list_op = "schema.list_models";
    let list_res = Resource::collection("__models");
    let ctx_a_list = create_context_with_user(list_op, list_res.clone(), user_a);
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
    let ctx_b_list = create_context_with_user(list_op, Resource::collection("__models"), user_b);
    let result_b = executor.execute(&ctx_b_list, &target_list, &meta, None).expect("User B list failed");

    if let singularity::execution::ExecutionResult::Read { data } = result_b {
        let list = data.as_array().expect("result should be array");
        assert!(!list.iter().any(|m| m["id"] == model_id), "User B should NOT see model_a");
        assert_eq!(list.len(), 0, "User B owns nothing, should see nothing");
    } else {
        panic!("expected read result");
    }

    // 4. User B tries to READ "model_a" directly -> Should be NOT FOUND (due to RLS)
    let read_op = "resource.read";
    let ctx_b_read = create_context_with_user(read_op, Resource::instance("__models", model_id), user_b);
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
    let ctx_a_read = create_context_with_user(read_op, Resource::instance("__models", model_id), user_a);
    let result_a_read = executor.execute(&ctx_a_read, &target_read, &meta, None);
    assert!(result_a_read.is_ok(), "User A should read model_a");
}
