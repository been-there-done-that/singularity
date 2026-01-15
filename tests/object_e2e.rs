use singularity::execution::{ExecutionContext, ExecutionMeta, ExecutionTarget, StateBackedExecutor, OperationExecutor, ExecutionResult};
use singularity::state::{SqliteState, State};
use singularity::protocol::{Resource, FieldSet, CapabilityPayload, opcode::*};
use singularity::capability::{CapabilitySigner, CapabilityVerifier, SigningKey};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

fn create_context(op: &str, resource: Resource) -> ExecutionContext {
    let signing_key = SigningKey::generate();
    let signer = CapabilitySigner::new(signing_key);
    let verifier = CapabilityVerifier::new(signer.verifying_key());

    let payload = CapabilityPayload::new(
        "cap-test",
        op,
        resource,
        FieldSet::all(),
        0,
        60,
    );
    let token = signer.mint(&payload).unwrap();
    let verified = verifier.verify(&token, 30).unwrap();
    ExecutionContext::new(verified)
}

#[test]
fn test_object_plane_end_to_end() {
    // 1. Setup State and Executor
    let mut db_path = std::env::temp_dir();
    db_path.push("object_e2e_test.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }
    
    let mut state = SqliteState::open(db_path.to_str().unwrap()).unwrap();
    
    // Run Migrations
    use singularity::migration::manager::MigrationManager;
    let migrations = MigrationManager::new();
    migrations.run(&mut state).expect("Failed to run migrations");

    let executor = StateBackedExecutor::new(&state);
    let meta = ExecutionMeta::new();

    // 2. Define Storage Root
    let mut storage_root = std::env::temp_dir();
    storage_root.push("singularity_test_objects");
    if storage_root.exists() {
        fs::remove_dir_all(&storage_root).unwrap();
    }
    fs::create_dir_all(&storage_root).unwrap();
    let root_str = storage_root.to_str().unwrap();

    // 3. Create Object Namespace ("photos")
    // We use resource.create on __object_namespaces
    let ns_id = "photos";
    let ctx_create_ns = create_context(RESOURCE_CREATE, Resource::instance("__object_namespaces", ns_id));
    let target_create_ns = ExecutionTarget::new(Resource::instance("__object_namespaces", ns_id));
    
    let ns_payload = json!({
        "name": "User Photos",
        "backend": "local",
        "owner_id": "test-user",
        "root_path": root_str,
        "created_at": 1234567890
    });
    
    executor.execute(&ctx_create_ns, &target_create_ns, &meta, Some(ns_payload)).expect("Failed to create namespace");

    // 4. Write Object ("vacation.jpg")
    let object_key = "vacation.jpg";
    let content = "Hello Object World";
    use base64::Engine;
    let valid_b64 = base64::engine::general_purpose::STANDARD.encode(content);
    
    let ctx_write = create_context(OBJECT_WRITE, Resource::instance(ns_id, object_key));
    let target_write = ExecutionTarget::new(Resource::instance(ns_id, object_key));
    
    executor.execute(&ctx_write, &target_write, &meta, Some(json!({ "data": valid_b64 })))
        .expect("Failed to write object");

    // Verify file exists on disk
    let file_path = storage_root.join(object_key);
    assert!(file_path.exists(), "File should exist on disk");
    assert_eq!(fs::read_to_string(&file_path).unwrap(), content);

    // 5. Read Object
    let ctx_read = create_context(OBJECT_READ, Resource::instance(ns_id, object_key));
    let target_read = ExecutionTarget::new(Resource::instance(ns_id, object_key));
    
    let read_result = executor.execute(&ctx_read, &target_read, &meta, None).expect("Failed to read object");
    
    if let ExecutionResult::Read { data } = read_result {
        let b64_out = data.get("data").unwrap().as_str().unwrap();
        let bytes_out = base64::engine::general_purpose::STANDARD.decode(b64_out).unwrap();
        assert_eq!(String::from_utf8(bytes_out).unwrap(), content);
    } else {
        panic!("Expected Read result");
    }

    // 6. List Objects
    let ctx_list = create_context(OBJECT_LIST, Resource::instance(ns_id, ""));
    let target_list = ExecutionTarget::new(Resource::instance(ns_id, "")); // empty ID = root prefix
    
    let list_result = executor.execute(&ctx_list, &target_list, &meta, None).expect("Failed to list objects");
    
    if let ExecutionResult::Read { data } = list_result {
        let list = data.as_array().expect("Expected array");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["key"], object_key);
        assert_eq!(list[0]["size"], content.len() as u64);
    } else {
        panic!("Expected Read result");
    }

    // 7. Delete Object
    let ctx_delete = create_context(OBJECT_DELETE, Resource::instance(ns_id, object_key));
    let target_delete = ExecutionTarget::new(Resource::instance(ns_id, object_key));
    
    executor.execute(&ctx_delete, &target_delete, &meta, None).expect("Failed to delete object");
    assert!(!file_path.exists(), "File should be deleted");

    // Cleanup
    let _ = fs::remove_dir_all(&storage_root);
    let _ = fs::remove_file(&db_path);
}
