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
    let signing_key = SigningKey::generate();
    let signer = CapabilitySigner::new(signing_key);
    let verifier = CapabilityVerifier::new(signer.verifying_key());
    let payload = CapabilityPayload::new("test", op, resource, FieldSet::all(), 0, 60);
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
    } else {
        panic!("expected read result after drop");
    }
}
