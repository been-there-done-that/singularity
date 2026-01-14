//! State-agnostic conformance tests.
//!
//! All state backends MUST pass these tests.
//! SQLite defines semantic truth.

use serde_json::json;

use crate::execution::ExecutionTarget;
use crate::protocol::{FieldSet, Resource};

use super::error::StateError;
use super::traits::State;

/// Create a target from resource type and optional ID.
fn target(resource_type: &str, id: Option<&str>) -> ExecutionTarget {
    let resource = match id {
        Some(id) => Resource::instance(resource_type, id),
        None => Resource::collection(resource_type),
    };
    ExecutionTarget::new(resource)
}

/// Run all conformance tests on a state implementation.
///
/// All backends must pass these tests to be considered compliant.
pub fn run_conformance_tests<S: State>(state: &S) {
    conformance_read_nonexistent(state);
    conformance_write_then_read(state);
    conformance_field_filtering(state);
    conformance_cas_version_success(state);
    conformance_cas_version_failure(state);
    conformance_delete_existing(state);
    conformance_delete_nonexistent(state);
    conformance_capabilities_exposed(state);
}

fn conformance_read_nonexistent<S: State>(state: &S) {
    let t = target("conformance_test", Some("nonexistent_id"));
    let result = state.read(&t, &FieldSet::all(), None);
    assert!(
        matches!(result, Err(StateError::NotFound { .. })),
        "read of nonexistent resource must return NotFound"
    );
}

fn conformance_write_then_read<S: State>(state: &S) {
    let t = target("conformance_test", Some("write_read_id"));
    let payload = json!({"field1": "value1", "field2": 42});
    
    state.write(&t, &FieldSet::all(), &payload, None)
        .expect("write must succeed");
    
    let result = state.read(&t, &FieldSet::all(), None)
        .expect("read after write must succeed");
    
    assert_eq!(result.get("field1"), Some(&json!("value1")));
    assert_eq!(result.get("field2"), Some(&json!(42)));
}

fn conformance_field_filtering<S: State>(state: &S) {
    let t = target("conformance_test", Some("filter_id"));
    let payload = json!({"visible": "yes", "secret": "no"});
    
    state.write(&t, &FieldSet::all(), &payload, None).unwrap();
    
    let fields = FieldSet::new(["visible"]);
    let result = state.read(&t, &fields, None).unwrap();
    
    assert!(result.get("visible").is_some(), "visible field must be present");
    assert!(result.get("secret").is_none(), "secret field must be filtered");
}

fn conformance_cas_version_success<S: State>(state: &S) {
    if !state.capabilities().cas_constraints {
        return; // Skip if not supported
    }

    let t = target("conformance_test", Some("cas_success_id"));
    state.write(&t, &FieldSet::all(), &json!({"v": 1}), None).unwrap();
    
    let constraints = json!({"version_eq": 1});
    let result = state.write(&t, &FieldSet::all(), &json!({"v": 2}), Some(&constraints));
    assert!(result.is_ok(), "CAS with correct version must succeed");
}

fn conformance_cas_version_failure<S: State>(state: &S) {
    if !state.capabilities().cas_constraints {
        return; // Skip if not supported
    }

    let t = target("conformance_test", Some("cas_failure_id"));
    state.write(&t, &FieldSet::all(), &json!({"v": 1}), None).unwrap();
    
    let constraints = json!({"version_eq": 999});
    let result = state.write(&t, &FieldSet::all(), &json!({"v": 2}), Some(&constraints));
    assert!(
        matches!(result, Err(StateError::ConstraintViolation { .. })),
        "CAS with wrong version must return ConstraintViolation"
    );
}

fn conformance_delete_existing<S: State>(state: &S) {
    let t = target("conformance_test", Some("delete_existing_id"));
    state.write(&t, &FieldSet::all(), &json!({"data": "x"}), None).unwrap();
    
    let deleted = state.delete(&t, None).expect("delete must succeed");
    assert_eq!(deleted, 1, "delete must return affected count of 1");
    
    let result = state.read(&t, &FieldSet::all(), None);
    assert!(
        matches!(result, Err(StateError::NotFound { .. })),
        "read after delete must return NotFound"
    );
}

fn conformance_delete_nonexistent<S: State>(state: &S) {
    let t = target("conformance_test", Some("delete_nonexistent_id"));
    let result = state.delete(&t, None);
    assert!(
        matches!(result, Err(StateError::NotFound { .. })),
        "delete of nonexistent resource must return NotFound"
    );
}

fn conformance_capabilities_exposed<S: State>(state: &S) {
    let caps = state.capabilities();
    assert!(!caps.backend_name.is_empty(), "backend_name must be set");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::sqlite::SqliteState;

    #[test]
    fn test_sqlite_passes_conformance() {
        let state = SqliteState::in_memory().unwrap();
        run_conformance_tests(&state);
    }
}
