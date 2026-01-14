//! State-backed executor - the bridge between execution and state.
//!
//! # Purpose
//!
//! This is the **only** adapter between execution and state.
//! Execution knows the semantic interface, not the backend.
//!
//! # Invariants
//!
//! All execution invariants pass through here unchanged:
//! - No execution without VerifiedCapability
//! - No write field widening
//! - No backend-specific logic in execution

use crate::state::{State, StateError};

use super::context::{ExecutionContext, ExecutionMeta, ExecutionTarget};
use super::executor::OperationExecutor;
use super::result::{ExecutionError, ExecutionResult};

/// State-backed operation executor.
///
/// Implements `OperationExecutor` by delegating to a `State` backend.
/// This is the **final mechanical joint** between execution and storage.
pub struct StateBackedExecutor<'a, S: State> {
    state: &'a S,
}

impl<'a, S: State> StateBackedExecutor<'a, S> {
    /// Create a new state-backed executor.
    pub fn new(state: &'a S) -> Self {
        Self { state }
    }

    /// Get the underlying state backend.
    pub fn state(&self) -> &S {
        self.state
    }
}

/// Lossless semantic mapping from StateError to ExecutionError.
impl From<StateError> for ExecutionError {
    fn from(err: StateError) -> Self {
        match err {
            StateError::NotFound { resource_type, resource_id } => {
                ExecutionError::ResourceNotFound { resource_type, resource_id }
            }
            StateError::ConstraintViolation { constraint, reason } => {
                ExecutionError::ConstraintViolation { constraint, reason }
            }
            StateError::CapabilityNotSupported(cap) => {
                ExecutionError::BackendCapabilityMissing(cap)
            }
            StateError::ConnectionError(e) => {
                ExecutionError::StorageError(e)
            }
            StateError::InternalError(e) => {
                ExecutionError::StorageError(e)
            }
        }
    }
}

impl<'a, S: State> OperationExecutor for StateBackedExecutor<'a, S> {
    fn execute(
        &self,
        ctx: &ExecutionContext,
        target: &ExecutionTarget,
        _meta: &ExecutionMeta,
        payload: Option<serde_json::Value>,
    ) -> Result<ExecutionResult, ExecutionError> {
        // 1. Validate target against capability
        target.validate_against_context(ctx)?;

        // 2. Extract constraints from capability (if any)
        // For now, constraints come via payload for backwards compat
        // In full impl, they'd come from CapabilityPayload.constraints
        let constraints: Option<serde_json::Value> = None;

        // 3. Dispatch by opcode
        let op = ctx.op().as_str();

        match op {
            // READ operations
            "resource.read" | "user.read" | "document.read" => {
                let data = self.state.read(
                    target,
                    ctx.fields(),
                    constraints.as_ref(),
                )?;

                // 6. Filter output to authorized fields
                let filtered = ctx.filter_output(data);
                Ok(ExecutionResult::read(filtered))
            }

            // CREATE operations
            "resource.create" | "user.create" | "document.create" => {
                let payload = payload.ok_or_else(|| ExecutionError::ConstraintViolation {
                    constraint: "payload".to_string(),
                    reason: "create requires payload".to_string(),
                })?;

                // 2. Validate write fields BEFORE state
                ctx.validate_write_fields(&payload)?;

                // Delegate to state
                let count = self.state.write(
                    target,
                    ctx.fields(),
                    &payload,
                    constraints.as_ref(),
                )?;

                Ok(ExecutionResult::write(count))
            }

            // UPDATE operations
            "resource.update" | "user.update" | "document.update" => {
                let payload = payload.ok_or_else(|| ExecutionError::ConstraintViolation {
                    constraint: "payload".to_string(),
                    reason: "update requires payload".to_string(),
                })?;

                // 2. Validate write fields BEFORE state
                ctx.validate_write_fields(&payload)?;

                // Delegate to state
                let count = self.state.write(
                    target,
                    ctx.fields(),
                    &payload,
                    constraints.as_ref(),
                )?;

                Ok(ExecutionResult::write(count))
            }

            // DELETE operations
            "resource.delete" | "user.delete" | "document.delete" => {
                let count = self.state.delete(
                    target,
                    constraints.as_ref(),
                )?;

                Ok(ExecutionResult::write(count))
            }

            // Unknown operation
            _ => Err(ExecutionError::OperationNotSupported(op.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilitySigner, CapabilityVerifier, SigningKey};
    use crate::protocol::{CapabilityPayload, FieldSet, Resource};
    use crate::state::SqliteState;
    use serde_json::json;

    fn create_context(op: &str, resource: Resource, fields: FieldSet) -> ExecutionContext {
        let signing_key = SigningKey::generate();
        let signer = CapabilitySigner::new(signing_key);
        let verifier = CapabilityVerifier::new(signer.verifying_key());

        let payload = CapabilityPayload::new(
            "cap-test",
            op,
            resource,
            fields,
            1704067200,
            1704067260,
        );
        let token = signer.mint(&payload).unwrap();
        let verified = verifier.verify(&token, 1704067230).unwrap();
        ExecutionContext::new(verified)
    }

    #[test]
    fn test_state_backed_executor_write_then_read() {
        let state = SqliteState::in_memory().unwrap();
        let executor = StateBackedExecutor::new(&state);
        let meta = ExecutionMeta::new();

        // Write
        let ctx = create_context(
            "resource.create",
            Resource::instance("user", "test-123"),
            FieldSet::all(),
        );
        let target = ExecutionTarget::new(Resource::instance("user", "test-123"));
        let write_result = executor.execute(
            &ctx,
            &target,
            &meta,
            Some(json!({"name": "Alice", "email": "alice@example.com"})),
        );
        assert!(write_result.is_ok());

        // Read
        let ctx = create_context(
            "resource.read",
            Resource::instance("user", "test-123"),
            FieldSet::all(),
        );
        let read_result = executor.execute(&ctx, &target, &meta, None);
        assert!(read_result.is_ok());

        if let ExecutionResult::Read { data } = read_result.unwrap() {
            assert_eq!(data.get("name"), Some(&json!("Alice")));
        } else {
            panic!("Expected Read result");
        }
    }

    #[test]
    fn test_state_backed_executor_field_filtering() {
        let state = SqliteState::in_memory().unwrap();
        let executor = StateBackedExecutor::new(&state);
        let meta = ExecutionMeta::new();

        // Create with all fields
        let ctx = create_context(
            "resource.create",
            Resource::instance("user", "filter-test"),
            FieldSet::all(),
        );
        let target = ExecutionTarget::new(Resource::instance("user", "filter-test"));
        executor.execute(
            &ctx,
            &target,
            &meta,
            Some(json!({"name": "Bob", "email": "bob@test.com", "password": "secret"})),
        ).unwrap();

        // Read with limited fields
        let ctx = create_context(
            "resource.read",
            Resource::instance("user", "filter-test"),
            FieldSet::new(["name", "email"]),
        );
        let read_result = executor.execute(&ctx, &target, &meta, None).unwrap();

        if let ExecutionResult::Read { data } = read_result {
            assert!(data.get("name").is_some());
            assert!(data.get("email").is_some());
            assert!(data.get("password").is_none()); // Filtered!
        } else {
            panic!("Expected Read result");
        }
    }

    #[test]
    fn test_state_backed_executor_write_validation() {
        let state = SqliteState::in_memory().unwrap();
        let executor = StateBackedExecutor::new(&state);
        let meta = ExecutionMeta::new();

        // Try to write with limited fields
        let ctx = create_context(
            "resource.create",
            Resource::instance("user", "validate-test"),
            FieldSet::new(["name"]),
        );
        let target = ExecutionTarget::new(Resource::instance("user", "validate-test"));
        
        // Try to write unauthorized field
        let result = executor.execute(
            &ctx,
            &target,
            &meta,
            Some(json!({"name": "Charlie", "password": "sneaky"})),
        );

        assert!(matches!(result, Err(ExecutionError::UnauthorizedFieldWrite { .. })));
    }

    #[test]
    fn test_state_backed_executor_delete() {
        let state = SqliteState::in_memory().unwrap();
        let executor = StateBackedExecutor::new(&state);
        let meta = ExecutionMeta::new();

        // Create
        let ctx = create_context(
            "resource.create",
            Resource::instance("user", "delete-test"),
            FieldSet::all(),
        );
        let target = ExecutionTarget::new(Resource::instance("user", "delete-test"));
        executor.execute(&ctx, &target, &meta, Some(json!({"name": "Delete Me"}))).unwrap();

        // Delete
        let ctx = create_context(
            "resource.delete",
            Resource::instance("user", "delete-test"),
            FieldSet::all(),
        );
        let delete_result = executor.execute(&ctx, &target, &meta, None);
        assert!(delete_result.is_ok());

        // Read after delete should fail
        let ctx = create_context(
            "resource.read",
            Resource::instance("user", "delete-test"),
            FieldSet::all(),
        );
        let read_result = executor.execute(&ctx, &target, &meta, None);
        assert!(matches!(read_result, Err(ExecutionError::ResourceNotFound { .. })));
    }

    #[test]
    fn test_state_backed_executor_unsupported_op() {
        let state = SqliteState::in_memory().unwrap();
        let executor = StateBackedExecutor::new(&state);
        let meta = ExecutionMeta::new();

        let ctx = create_context(
            "unknown.operation",
            Resource::instance("user", "test"),
            FieldSet::all(),
        );
        let target = ExecutionTarget::new(Resource::instance("user", "test"));
        let result = executor.execute(&ctx, &target, &meta, None);

        assert!(matches!(result, Err(ExecutionError::OperationNotSupported(_))));
    }
}
