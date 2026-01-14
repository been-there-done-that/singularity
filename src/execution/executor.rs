//! Operation executor trait and in-memory test implementation.
//!
//! # Invariant
//!
//! All execution is driven exclusively by `VerifiedCapability` (via `ExecutionContext`):
//!
//! - Reads return ONLY authorized fields
//! - Writes must reference ONLY authorized fields (rejected, not dropped)
//! - Resource identity must match exactly
//! - Constraints must be validated before mutation
//! - No request metadata influences authorization

use super::context::{ExecutionContext, ExecutionMeta, ExecutionTarget};
use super::result::{ExecutionError, ExecutionResult};

/// Operation executor trait.
///
/// Implementations handle specific storage backends (SQLite, in-memory, etc.).
///
/// All methods require `ExecutionContext` (which requires `VerifiedCapability`).
/// There is NO execution path without verified authority.
pub trait OperationExecutor {
    /// Execute an operation.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Execution context (contains verified capability)
    /// * `target` - Explicit resource target
    /// * `meta` - Observability metadata (NOT authority)
    /// * `payload` - Operation payload (validated against ctx.fields)
    ///
    /// # Execution Order
    ///
    /// 1. Validate target against context
    /// 2. Validate constraints (if present)
    /// 3. Validate write fields (for mutations)
    /// 4. Execute operation
    /// 5. Filter output (for reads)
    fn execute(
        &self,
        ctx: &ExecutionContext,
        target: &ExecutionTarget,
        meta: &ExecutionMeta,
        payload: Option<serde_json::Value>,
    ) -> Result<ExecutionResult, ExecutionError>;
}

/// In-memory executor for testing.
///
/// This is a fake implementation for testing execution invariants.
/// It uses a simple hashmap as storage.
#[cfg(test)]
pub mod test_executor {
    use super::*;
    use crate::execution::constraint::validate_constraints;
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    /// In-memory storage for testing.
    pub type TestStorage = Arc<RwLock<HashMap<String, serde_json::Value>>>;

    /// Simple in-memory executor for testing.
    pub struct InMemoryExecutor {
        storage: TestStorage,
    }

    impl InMemoryExecutor {
        pub fn new() -> Self {
            Self {
                storage: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        pub fn with_storage(storage: TestStorage) -> Self {
            Self { storage }
        }

        /// Insert test data directly (bypasses capability for test setup).
        pub fn insert_test_data(&self, key: &str, value: serde_json::Value) {
            self.storage.write().unwrap().insert(key.to_string(), value);
        }

        fn storage_key(target: &ExecutionTarget) -> String {
            match &target.resource.resource_id {
                Some(id) => format!("{}:{}", target.resource.resource_type, id),
                None => target.resource.resource_type.clone(),
            }
        }
    }

    impl OperationExecutor for InMemoryExecutor {
        fn execute(
            &self,
            ctx: &ExecutionContext,
            target: &ExecutionTarget,
            _meta: &ExecutionMeta,
            payload: Option<serde_json::Value>,
        ) -> Result<ExecutionResult, ExecutionError> {
            // 1. Validate target against context
            target.validate_against_context(ctx)?;

            let key = Self::storage_key(target);
            let storage = self.storage.read().unwrap();
            let current_state = storage.get(&key).cloned();
            drop(storage);

            // Get constraints from capability (via payload for this test impl)
            // In real impl, constraints come from CapabilityPayload
            let constraints = ctx
                .fields()
                .fields
                .iter()
                .find(|f| f.starts_with("__constraint:"))
                .map(|_| serde_json::json!({})); // Simplified for testing

            // 2. Validate constraints
            validate_constraints(constraints.as_ref(), current_state.as_ref())?;

            // Determine operation type
            let op = ctx.op().as_str();

            match op {
                "resource.read" | "user.read" | "document.read" => {
                    // READ operation
                    match current_state {
                        Some(data) => {
                            // 5. Filter output to authorized fields
                            let filtered = ctx.filter_output(data);
                            Ok(ExecutionResult::read(filtered))
                        }
                        None => Err(ExecutionError::ResourceNotFound {
                            resource_type: target.resource.resource_type.clone(),
                            resource_id: target.resource.resource_id.clone().unwrap_or_default(),
                        }),
                    }
                }
                "resource.create" | "user.create" | "document.create" => {
                    // CREATE operation
                    let payload = payload.ok_or_else(|| ExecutionError::ConstraintViolation {
                        constraint: "payload".to_string(),
                        reason: "create requires payload".to_string(),
                    })?;

                    // 3. Validate write fields
                    ctx.validate_write_fields(&payload)?;

                    // 4. Execute
                    let mut storage = self.storage.write().unwrap();
                    storage.insert(key, payload);

                    Ok(ExecutionResult::write(1))
                }
                "resource.update" | "user.update" | "document.update" => {
                    // UPDATE operation
                    let payload = payload.ok_or_else(|| ExecutionError::ConstraintViolation {
                        constraint: "payload".to_string(),
                        reason: "update requires payload".to_string(),
                    })?;

                    // 3. Validate write fields
                    ctx.validate_write_fields(&payload)?;

                    // Check exists
                    if current_state.is_none() {
                        return Err(ExecutionError::ResourceNotFound {
                            resource_type: target.resource.resource_type.clone(),
                            resource_id: target.resource.resource_id.clone().unwrap_or_default(),
                        });
                    }

                    // 4. Execute (merge with existing)
                    let mut storage = self.storage.write().unwrap();
                    if let Some(existing) = storage.get_mut(&key) {
                        if let (Some(existing_obj), Some(payload_obj)) =
                            (existing.as_object_mut(), payload.as_object())
                        {
                            for (k, v) in payload_obj {
                                existing_obj.insert(k.clone(), v.clone());
                            }
                        }
                    }

                    Ok(ExecutionResult::write(1))
                }
                "resource.delete" | "user.delete" | "document.delete" => {
                    // DELETE operation
                    let mut storage = self.storage.write().unwrap();

                    if storage.remove(&key).is_some() {
                        Ok(ExecutionResult::write(1))
                    } else {
                        Err(ExecutionError::ResourceNotFound {
                            resource_type: target.resource.resource_type.clone(),
                            resource_id: target.resource.resource_id.clone().unwrap_or_default(),
                        })
                    }
                }
                _ => Err(ExecutionError::OperationNotSupported(op.to_string())),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::capability::{CapabilitySigner, CapabilityVerifier, SigningKey};
        use crate::protocol::{CapabilityPayload, FieldSet, Resource};
        use serde_json::json;

        fn create_context(
            op: &str,
            resource: Resource,
            fields: FieldSet,
        ) -> ExecutionContext {
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
        fn test_executor_read_filters_fields() {
            let executor = InMemoryExecutor::new();
            executor.insert_test_data(
                "user:123",
                json!({
                    "id": "123",
                    "name": "Alice",
                    "email": "alice@example.com",
                    "password_hash": "secret"
                }),
            );

            let ctx = create_context(
                "resource.read",
                Resource::instance("user", "123"),
                FieldSet::new(["name", "email"]),
            );
            let target = ExecutionTarget::new(Resource::instance("user", "123"));
            let meta = ExecutionMeta::new();

            let result = executor.execute(&ctx, &target, &meta, None).unwrap();

            if let ExecutionResult::Read { data } = result {
                assert!(data.get("name").is_some());
                assert!(data.get("email").is_some());
                assert!(data.get("password_hash").is_none()); // Filtered!
            } else {
                panic!("Expected Read result");
            }
        }

        #[test]
        fn test_executor_write_rejects_unauthorized_field() {
            let executor = InMemoryExecutor::new();

            let ctx = create_context(
                "resource.create",
                Resource::instance("user", "123"),
                FieldSet::new(["name", "email"]),
            );
            let target = ExecutionTarget::new(Resource::instance("user", "123"));
            let meta = ExecutionMeta::new();

            let payload = json!({
                "name": "Alice",
                "password_hash": "trying_to_set_password"
            });

            let result = executor.execute(&ctx, &target, &meta, Some(payload));
            assert!(matches!(result, Err(ExecutionError::UnauthorizedFieldWrite { .. })));
        }

        #[test]
        fn test_executor_requires_matching_resource() {
            let executor = InMemoryExecutor::new();

            let ctx = create_context(
                "resource.read",
                Resource::instance("user", "123"),
                FieldSet::all(),
            );
            // Wrong resource ID
            let target = ExecutionTarget::new(Resource::instance("user", "456"));
            let meta = ExecutionMeta::new();

            let result = executor.execute(&ctx, &target, &meta, None);
            assert!(result.is_err());
        }

        #[test]
        fn test_executor_create_update_delete_flow() {
            let executor = InMemoryExecutor::new();
            let meta = ExecutionMeta::new();

            // CREATE
            let ctx = create_context(
                "resource.create",
                Resource::instance("user", "new-id"),
                FieldSet::all(),
            );
            let target = ExecutionTarget::new(Resource::instance("user", "new-id"));
            let create_result = executor.execute(
                &ctx,
                &target,
                &meta,
                Some(json!({"name": "Bob", "email": "bob@example.com"})),
            );
            assert!(create_result.is_ok());
            assert!(matches!(create_result.unwrap(), ExecutionResult::Write { affected_count: 1 }));

            // READ
            let ctx = create_context(
                "resource.read",
                Resource::instance("user", "new-id"),
                FieldSet::all(),
            );
            let read_result = executor.execute(&ctx, &target, &meta, None);
            assert!(read_result.is_ok());

            // UPDATE
            let ctx = create_context(
                "resource.update",
                Resource::instance("user", "new-id"),
                FieldSet::new(["name"]),
            );
            let update_result = executor.execute(
                &ctx,
                &target,
                &meta,
                Some(json!({"name": "Bobby"})),
            );
            assert!(update_result.is_ok());

            // DELETE
            let ctx = create_context(
                "resource.delete",
                Resource::instance("user", "new-id"),
                FieldSet::all(),
            );
            let delete_result = executor.execute(&ctx, &target, &meta, None);
            assert!(delete_result.is_ok());

            // READ after delete should fail
            let ctx = create_context(
                "resource.read",
                Resource::instance("user", "new-id"),
                FieldSet::all(),
            );
            let read_after_delete = executor.execute(&ctx, &target, &meta, None);
            assert!(matches!(read_after_delete, Err(ExecutionError::ResourceNotFound { .. })));
        }
    }
}
