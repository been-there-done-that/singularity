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
use crate::object::ObjectManager;
use crate::protocol::opcode::*;

use super::context::{ExecutionContext, ExecutionMeta, ExecutionTarget};
use super::executor::OperationExecutor;
use super::result::{ExecutionError, ExecutionResult};
use crate::planner;
use crate::executor as pap_executor;
use crate::protocol::data::{QueryInput, InsertInput, UpdateInput, DeleteInput, PlanGrant};


/// State-backed operation executor.
///
/// Implements `OperationExecutor` by delegating to a `State` backend.
/// This is the **final mechanical joint** between execution and storage.
pub struct StateBackedExecutor<'a, S: State> {
    state: &'a S,
    object_manager: ObjectManager,
}

impl<'a, S: State> StateBackedExecutor<'a, S> {
    /// Create a new state-backed executor.
    pub fn new(state: &'a S) -> Self {
        Self { 
            state,
            object_manager: ObjectManager::new(),
        }
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
            StateError::BadRequest(msg) => {
                ExecutionError::BadRequest(msg)
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
            RESOURCE_READ | USER_READ | DOCUMENT_READ => {
                // automatic owner filtering for strict mode
                let mut exec_constraints = serde_json::Map::new();
                if let Some(user_id) = ctx.internal_user_id() {
                     exec_constraints.insert("owner_id".to_string(), serde_json::json!(user_id));
                     println!("DEBUG: StateBackedExecutor::Read - OwnerID: {}", user_id);
                } else {
                     println!("DEBUG: StateBackedExecutor::Read - No OwnerID");
                }
                
                // Merge with detailed constraints from payload
                if let Some(p) = payload {
                    if let Some(obj) = p.as_object() {
                        for (k, v) in obj {
                            exec_constraints.insert(k.clone(), v.clone());
                        }
                    }
                }

                // Merge with explicit constraints if any (future proofing)
                let c_val = if !exec_constraints.is_empty() {
                    Some(serde_json::Value::Object(exec_constraints))
                } else {
                    None
                };

                let data = self.state.read(
                    target,
                    ctx.fields(),
                    c_val.as_ref(), // Passed combined constraints
                )?;

                // 6. Filter output to authorized fields
                let filtered = ctx.filter_output(data);
                Ok(ExecutionResult::read(filtered))
            }

            SCHEMA_LIST_MODELS => {
                 // Override target to read from __models system table
                let params = crate::protocol::Resource::collection("__models");
                let list_target = ExecutionTarget::new(params);

                // 1. Prepare constraints (Owner Filtering)
                let mut exec_constraints = serde_json::Map::new();
                if let Some(user_id) = ctx.internal_user_id() {
                     // Only enforce owner filtering for non-admins
                     if !ctx.has_role("admin") {
                        exec_constraints.insert("owner_id".to_string(), serde_json::json!(user_id));
                     }
                }

                if let Some(p) = payload {
                    if let Some(obj) = p.as_object() {
                        for (k, v) in obj {
                            exec_constraints.insert(k.clone(), v.clone());
                        }
                    }
                }
                let c_val = if !exec_constraints.is_empty() {
                    Some(serde_json::Value::Object(exec_constraints))
                } else {
                    None
                };

                // 2. Fetch Models
                let models = self.state.read(
                    &list_target,
                    ctx.fields(),
                    c_val.as_ref(),
                )?;

                // 3. Fetch Ownership Metadata (Enrichment)
                // Query __fields for any field named 'owner_id'
                let fields_params = crate::protocol::Resource::collection("__fields");
                let fields_target = ExecutionTarget::new(fields_params);
                let ownership_constraints = serde_json::json!({ "name": "owner_id" });
                
                let fields = self.state.read(
                    &fields_target, 
                    &crate::protocol::FieldSet::all(), 
                    Some(&ownership_constraints)
                ).unwrap_or(serde_json::Value::Array(vec![])); // Ignore error if __fields fail

                // Build Set of owned model_ids
                use std::collections::HashSet;
                let mut owned_models = HashSet::new();
                if let serde_json::Value::Array(list) = fields {
                    for f in list {
                        if let Some(mid) = f.get("model_id").and_then(|v| v.as_str()) {
                            owned_models.insert(mid.to_string());
                        }
                    }
                }

                // 4. Enrich
                let mut enriched = models;
                if let serde_json::Value::Array(ref mut list) = enriched {
                    for model in list {
                        if let Some(obj) = model.as_object_mut() {
                            if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
                                if owned_models.contains(id) {
                                    obj.insert("ownership".into(), serde_json::json!({
                                        "column": "owner_id",
                                        "principal": "__internal_users"
                                    }));
                                }
                            }
                        }
                    }
                }

                let filtered = ctx.filter_output(enriched);
                Ok(ExecutionResult::read(filtered))
            }

            SCHEMA_DESCRIBE => {
                let model_name = &target.resource.resource_type;
                
                // 1. Resolve Model Name -> ID
                let params_models = crate::protocol::Resource::collection("__models");
                let models_target = ExecutionTarget::new(params_models);
                let mut lookup_constraints = serde_json::Map::new();
                lookup_constraints.insert("name".to_string(), serde_json::json!(model_name));
                
                let models = self.state.read(
                    &models_target,
                    &crate::protocol::FieldSet::all(),
                    Some(&serde_json::Value::Object(lookup_constraints))
                )?;
                
                let model_id = if let serde_json::Value::Array(list) = models {
                     list.first().and_then(|m| m.get("id")).and_then(|v| v.as_str()).map(|s| s.to_string())
                } else {
                     None
                };
                
                let model_id = model_id.ok_or_else(|| ExecutionError::ResourceNotFound { 
                    resource_type: "Model".into(), 
                    resource_id: model_name.to_string() 
                })?;

                // 2. Describe (Query __fields)
                // Override target to read from __fields system table
                let params = crate::protocol::Resource::collection("__fields");
                let describe_target = ExecutionTarget::new(params);
                
                let mut exec_constraints = serde_json::Map::new();
                exec_constraints.insert("model_id".to_string(), serde_json::json!(model_id));
                
                if let Some(p) = payload {
                    if let Some(obj) = p.as_object() {
                        for (k, v) in obj {
                            // Don't allow overriding model_id
                            if k != "model_id" {
                                exec_constraints.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }

                let c_val = Some(serde_json::Value::Object(exec_constraints));

                let fields = self.state.read(
                    &describe_target,
                    &crate::protocol::FieldSet::all(), // Always read all field metadata
                    c_val.as_ref(),
                )?;
                
                // Return as { fields: [...] } to match plan
                Ok(ExecutionResult::read(serde_json::json!({ "fields": fields })))
            }



            // CREATE operations
            RESOURCE_CREATE | USER_CREATE | DOCUMENT_CREATE | 
            SCHEMA_CREATE_MODEL | SCHEMA_ADD_FIELD | SCHEMA_CREATE_INDEX => {
                let mut payload = payload.ok_or_else(|| ExecutionError::ConstraintViolation {
                    constraint: "payload".to_string(),
                    reason: "create requires payload".to_string(),
                })?;

                // 2. Validate write fields BEFORE system injection
                // This ensures clients only write what they are authorized to, 
                // while the system manages the metadata.
                ctx.validate_write_fields(&payload)?;

                // 3. System Managed Injection (Source of Truth)
                if let Some(obj) = payload.as_object_mut() {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    
                    // Always set timestamps
                    obj.insert("created_at".into(), serde_json::json!(now));
                    obj.insert("updated_at".into(), serde_json::json!(now));

                    // Enforcement: Always overwrite owner_id if we have a subject
                    if let Some(owner) = ctx.internal_user_id() {
                        obj.insert("owner_id".into(), serde_json::json!(owner));
                        println!("DEBUG: StateBackedExecutor::Write - Injected OwnerID: {}", owner);
                    } else {
                        println!("DEBUG: StateBackedExecutor::Write - No OwnerID to inject");
                    }
                }

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
            RESOURCE_UPDATE | USER_UPDATE | DOCUMENT_UPDATE => {
                let mut payload = payload.ok_or_else(|| ExecutionError::ConstraintViolation {
                    constraint: "payload".to_string(),
                    reason: "update requires payload".to_string(),
                })?;

                // Immutability: Remove owner_id from payload if present
                if let Some(obj) = payload.as_object_mut() {
                     obj.remove("owner_id");
                }

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
            RESOURCE_DELETE | USER_DELETE | DOCUMENT_DELETE |
            SCHEMA_DROP_FIELD | SCHEMA_DROP_INDEX => {
                // 1. Extract constraints from payload (if provided)
                let mut exec_constraints = serde_json::Map::new();
                if let Some(user_id) = ctx.internal_user_id() {
                     exec_constraints.insert("owner_id".to_string(), serde_json::json!(user_id));
                }
                if let Some(p) = payload {
                    if let Some(obj) = p.as_object() {
                        for (k, v) in obj {
                            exec_constraints.insert(k.clone(), v.clone());
                        }
                    }
                }
                let c_val = if !exec_constraints.is_empty() {
                    Some(serde_json::Value::Object(exec_constraints.clone()))
                } else {
                    None
                };

                // 2. Handle special case: schema.drop_field requires ID construction
                let mut target = target.clone();
                if op == SCHEMA_DROP_FIELD && target.resource.resource_id.is_none() {
                    let p = c_val.as_ref().ok_or(ExecutionError::BadRequest("drop_field requires payload".into()))?;
                    let model_id = p.get("model_id").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("missing model_id".into()))?;
                    let name = p.get("name").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("missing name".into()))?;
                    
                    let field_id = format!("{}-{}", model_id, name);
                    target.resource.resource_id = Some(field_id);
                }

                let count = self.state.delete(
                    &target,
                    c_val.as_ref(),
                )?;

                Ok(ExecutionResult::write(count))
            }

            // OBJECT operations
            OBJECT_READ => {
                let namespace_id = &target.resource.resource_type;
                let key = target.resource.resource_id.as_ref().ok_or(ExecutionError::BadRequest("Missing object key".into()))?;

                // 1. Resolve Namespace Metadata from State
                let ns_target = ExecutionTarget::new(crate::protocol::Resource::instance("__object_namespaces", namespace_id));
                let ns_meta = self.state.read(&ns_target, &crate::protocol::FieldSet::all(), None)
                    .map_err(|e| match e {
                         StateError::NotFound { .. } => ExecutionError::ResourceNotFound { resource_type: "Namespace".into(), resource_id: namespace_id.clone() },
                         _ => ExecutionError::from(e)
                    })?;

                let backend = ns_meta.get("backend").and_then(|v| v.as_str()).unwrap_or("local");
                let root_path = ns_meta.get("root_path").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Namespace missing root_path".into()))?;

                // 2. Get Store
                let store = self.object_manager.get_store(namespace_id, backend, root_path)
                    .map_err(|e| ExecutionError::StorageError(e.to_string()))?;

                // 3. Delegate to Store
                let data = store.read(key)
                    .map_err(|e| ExecutionError::StorageError(e.to_string()))?;

                // Return as Base64 for now
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(data);
                Ok(ExecutionResult::Read { data: serde_json::json!({ "data": b64 }) })
            }

            OBJECT_WRITE => {
                let namespace_id = &target.resource.resource_type;
                let key = target.resource.resource_id.as_ref().ok_or(ExecutionError::BadRequest("Missing object key".into()))?;
                let payload = payload.ok_or(ExecutionError::BadRequest("Missing payload".into()))?;
                
                let data_b64 = payload.get("data").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Missing data field".into()))?;
                
                use base64::Engine;
                let data = base64::engine::general_purpose::STANDARD.decode(data_b64)
                    .map_err(|e| ExecutionError::BadRequest(format!("Invalid base64: {}", e)))?;

                // 1. Resolve Namespace
                let ns_target = ExecutionTarget::new(crate::protocol::Resource::instance("__object_namespaces", namespace_id));
                let ns_meta = self.state.read(&ns_target, &crate::protocol::FieldSet::all(), None)
                     .map_err(|e| match e {
                         StateError::NotFound { .. } => ExecutionError::ResourceNotFound { resource_type: "Namespace".into(), resource_id: namespace_id.clone() },
                         _ => ExecutionError::from(e)
                    })?;
                
                let backend = ns_meta.get("backend").and_then(|v| v.as_str()).unwrap_or("local");
                let root_path = ns_meta.get("root_path").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Namespace missing root_path".into()))?;

                 // 2. Get Store
                let store = self.object_manager.get_store(namespace_id, backend, root_path)
                    .map_err(|e| ExecutionError::StorageError(e.to_string()))?;

                store.write(key, &data).map_err(|e| ExecutionError::StorageError(e.to_string()))?;
                
                Ok(ExecutionResult::Write { affected_count: 1 })
            }

            OBJECT_DELETE => {
                let namespace_id = &target.resource.resource_type;
                let key = target.resource.resource_id.as_ref().ok_or(ExecutionError::BadRequest("Missing object key".into()))?;

                 // 1. Resolve Namespace
                let ns_target = ExecutionTarget::new(crate::protocol::Resource::instance("__object_namespaces", namespace_id));
                let ns_meta = self.state.read(&ns_target, &crate::protocol::FieldSet::all(), None)
                     .map_err(|e| match e {
                         StateError::NotFound { .. } => ExecutionError::ResourceNotFound { resource_type: "Namespace".into(), resource_id: namespace_id.clone() },
                         _ => ExecutionError::from(e)
                    })?;
                
                let backend = ns_meta.get("backend").and_then(|v| v.as_str()).unwrap_or("local");
                let root_path = ns_meta.get("root_path").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Namespace missing root_path".into()))?;

                 // 2. Get Store
                let store = self.object_manager.get_store(namespace_id, backend, root_path)
                    .map_err(|e| ExecutionError::StorageError(e.to_string()))?;

                store.delete(key).map_err(|e| ExecutionError::StorageError(e.to_string()))?;
                
                Ok(ExecutionResult::Write { affected_count: 1 })
            }

            OBJECT_LIST => {
                 let namespace_id = &target.resource.resource_type;
                 let prefix = target.resource.resource_id.as_deref().unwrap_or("");
                 
                  // 1. Resolve Namespace
                let ns_target = ExecutionTarget::new(crate::protocol::Resource::instance("__object_namespaces", namespace_id));
                let ns_meta = self.state.read(&ns_target, &crate::protocol::FieldSet::all(), None)
                     .map_err(|e| match e {
                         StateError::NotFound { .. } => ExecutionError::ResourceNotFound { resource_type: "Namespace".into(), resource_id: namespace_id.clone() },
                         _ => ExecutionError::from(e)
                    })?;
                
                let backend = ns_meta.get("backend").and_then(|v| v.as_str()).unwrap_or("local");
                let root_path = ns_meta.get("root_path").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Namespace missing root_path".into()))?;

                 // 2. Get Store
                let store = self.object_manager.get_store(namespace_id, backend, root_path)
                    .map_err(|e| ExecutionError::StorageError(e.to_string()))?;
                
                let entries = store.list(prefix).map_err(|e| ExecutionError::StorageError(e.to_string()))?;
                
                // Map entries to JSON
                let json_entries: Vec<serde_json::Value> = entries.into_iter().map(|e| {
                    serde_json::json!({
                        "key": e.key,
                        "size": e.size,
                        "created_at": e.created_at
                    })
                }).collect();
                
                Ok(ExecutionResult::Read { data: serde_json::Value::Array(json_entries) })
            }

            OBJECT_PRESIGN => {
                 let namespace_id = &target.resource.resource_type;
                 let key = target.resource.resource_id.as_deref().ok_or(ExecutionError::BadRequest("Missing object key".into()))?;
                 let payload = payload.ok_or(ExecutionError::BadRequest("Missing payload".into()))?;
                 
                 let method = payload.get("method").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Missing method".into()))?;
                 let ttl_secs = payload.get("ttl").and_then(|v| v.as_u64()).unwrap_or(300);
                 
                 // 1. Resolve Namespace
                let ns_target = ExecutionTarget::new(crate::protocol::Resource::instance("__object_namespaces", namespace_id));
                let ns_meta = self.state.read(&ns_target, &crate::protocol::FieldSet::all(), None)
                     .map_err(|e| match e {
                         StateError::NotFound { .. } => ExecutionError::ResourceNotFound { resource_type: "Namespace".into(), resource_id: namespace_id.clone() },
                         _ => ExecutionError::from(e)
                    })?;
                
                let backend = ns_meta.get("backend").and_then(|v| v.as_str()).unwrap_or("local");
                let root_path = ns_meta.get("root_path").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Namespace missing root_path".into()))?;

                 // 2. Get Store
                let store = self.object_manager.get_store(namespace_id, backend, root_path)
                    .map_err(|e| ExecutionError::StorageError(e.to_string()))?;
                
                // 3. Delegate Presign
                use std::time::Duration;
                let ttl = Duration::from_secs(ttl_secs);
                
                let presigned = match method {
                    "PUT" => store.presign_put(key, ttl),
                    "GET" => store.presign_get(key, ttl),
                    _ => return Err(ExecutionError::BadRequest("Invalid method for presign".into())),
                }.map_err(|e| ExecutionError::StorageError(e.to_string()))?;
                
                Ok(ExecutionResult::Read { 
                    data: serde_json::json!(presigned) 
                })
            }

            // COUNT operations
            RESOURCE_COUNT => {
                // Owner filtering constraints
                let mut exec_constraints = serde_json::Map::new();
                if let Some(user_id) = ctx.internal_user_id() {
                     exec_constraints.insert("owner_id".to_string(), serde_json::json!(user_id));
                }
                if let Some(p) = payload {
                    if let Some(obj) = p.as_object() {
                        for (k, v) in obj {
                            exec_constraints.insert(k.clone(), v.clone());
                        }
                    }
                }
                let c_val = if !exec_constraints.is_empty() {
                    Some(serde_json::Value::Object(exec_constraints))
                } else {
                    None
                };

                let count = self.state.count(target, c_val.as_ref())?;
                Ok(ExecutionResult::Read { 
                    data: serde_json::json!({ "count": count }) 
                })
            }

            // SCHEMA RENAME operations
            SCHEMA_RENAME_MODEL => {
                let payload = payload.ok_or(ExecutionError::BadRequest("Missing payload".into()))?;
                let old_name = payload.get("old_name").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Missing old_name".into()))?;
                let new_name = payload.get("new_name").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Missing new_name".into()))?;
                
                // Idempotency: if already renamed (old doesn't exist, new exists), consider success?
                // Or let State handle it? State returns NotFound if old doesn't exist.
                // User requirement: "renaming to same name -> no-op"
                if old_name == new_name {
                    return Ok(ExecutionResult::NoOp);
                }

                // Check ownership/permissions?
                // Schema ops usually need admin or ownership. 
                // Capability should enforce this. If user has capability, they can do it.
                // Strict: check if user owns the model? System models?
                // For v0, explicit capability is enough.

                match self.state.rename_table(old_name, new_name) {
                    Ok(_) => Ok(ExecutionResult::NoOp),
                    Err(StateError::NotFound { .. }) => {
                        // Check if new_name exists (already renamed?)
                         let _check_target = ExecutionTarget::new(crate::protocol::Resource::collection(new_name));
                         // If we can count it, it exists? Or check __models.
                         // Use __models read
                         let _model_target = ExecutionTarget::new(crate::protocol::Resource::instance("__models", new_name)); // ID is usually same as name or UUID? 
                         // Wait, in write implementation: `id` is passed.
                         // `rename_table` uses `name`.
                         // `State::rename_table` implementation checks `__models WHERE name = ?`. 
                         
                         // If we really want to support idempotency efficiently, we'd need to know if the failure was because 
                         // "From" doesn't exist. If "From" missing, maybe "To" exists?
                         // For now, let's just propagate error. The UI can handle 404.
                         // Or we can do a check.
                         Err(ExecutionError::ResourceNotFound { resource_type: "Model".into(), resource_id: old_name.into() })
                    },
                    Err(e) => Err(ExecutionError::from(e)),
                }
            }

            SCHEMA_RENAME_FIELD => {
                let payload = payload.ok_or(ExecutionError::BadRequest("Missing payload".into()))?;
                let table = payload.get("model").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Missing model".into()))?;
                let old_col = payload.get("old_name").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Missing old_name".into()))?;
                let new_col = payload.get("new_name").and_then(|v| v.as_str()).ok_or(ExecutionError::BadRequest("Missing new_name".into()))?;

                if old_col == new_col {
                     return Ok(ExecutionResult::NoOp);
                }

                match self.state.rename_column(table, old_col, new_col) {
                    Ok(_) => Ok(ExecutionResult::NoOp),
                    // Handle idempotency similar to rename_model if needed
                     Err(StateError::NotFound { .. }) => {
                         Err(ExecutionError::ResourceNotFound { resource_type: "Field".into(), resource_id: old_col.into() })
                     },
                     Err(e) => Err(ExecutionError::from(e)),
                }
            }

            DATA_INSERT => {
                let payload = payload.ok_or(ExecutionError::BadRequest("Missing insert input".into()))?;
                let mut input: InsertInput = serde_json::from_value(payload)
                    .map_err(|e| ExecutionError::BadRequest(e.to_string()))?;
                
                // Inject system fields (ID, timestamps, owner)
                // Note: We generate fresh IDs here. 
                // This assumes plan hashing ignores literals (or allows this).
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                for row in &mut input.rows {
                     if let serde_json::Value::Object(map) = row {
                         if !map.contains_key("id") {
                             map.insert("id".into(), serde_json::Value::String(uuid::Uuid::new_v4().to_string()));
                         }
                         if !map.contains_key("created_at") {
                             map.insert("created_at".into(), serde_json::json!(now));
                         }
                         if !map.contains_key("updated_at") {
                             map.insert("updated_at".into(), serde_json::json!(now));
                         }
                         if !map.contains_key("owner_id") {
                             // Use verified internal ID
                             if let Some(iid) = ctx.internal_user_id() {
                                  map.insert("owner_id".into(), serde_json::Value::String(iid.to_string()));
                             }
                         }
                     }
                }

                let model_name = &target.resource.resource_type;
                
                let plan = planner::plan_insert(model_name, &input, self.state)
                    .map_err(|e| ExecutionError::BadRequest(e.to_string()))?;

                let constraints_val = ctx.capability().payload().constraints.as_ref()
                    .ok_or(ExecutionError::BadRequest("Capacity missing grant constraints".into()))?;
                let grant: PlanGrant = serde_json::from_value(constraints_val.clone())
                    .map_err(|e| ExecutionError::BadRequest(format!("Invalid grant in token: {}", e)))?;
                
                let expected_hash = ctx.capability().payload().plan_hash.as_deref();
                pap_executor::verify_plan_hash(expected_hash, &plan, &grant) 
                     .map_err(|e| ExecutionError::ConstraintViolation { constraint: "plan_hash".into(), reason: e.to_string() })?;

                 self.state.execute_plan(&plan, &grant, ctx.internal_user_id())
                     .map_err(ExecutionError::from)
            }

            DATA_QUERY | DATA_COUNT => {
                let payload = payload.ok_or(ExecutionError::BadRequest("Missing query input".into()))?;
                let input: QueryInput = serde_json::from_value(payload)
                    .map_err(|e| ExecutionError::BadRequest(e.to_string()))?;
                let model_name = &target.resource.resource_type;
                
                let plan = if op == DATA_QUERY {
                     planner::plan_query(model_name, &input, self.state)
                } else {
                     planner::plan_count(model_name, &input, self.state)
                }.map_err(|e| ExecutionError::BadRequest(e.to_string()))?;

                let constraints_val = ctx.capability().payload().constraints.as_ref()
                    .ok_or(ExecutionError::BadRequest("Capacity missing grant constraints".into()))?;
                let grant: PlanGrant = serde_json::from_value(constraints_val.clone())
                    .map_err(|e| ExecutionError::BadRequest(format!("Invalid grant in token: {}", e)))?;
                
                let expected_hash = ctx.capability().payload().plan_hash.as_deref();
                pap_executor::verify_plan_hash(expected_hash, &plan, &grant) 
                     .map_err(|e| ExecutionError::ConstraintViolation { constraint: "plan_hash".into(), reason: e.to_string() })?;

                 self.state.execute_plan(&plan, &grant, ctx.internal_user_id())
                     .map_err(ExecutionError::from)
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
            RESOURCE_CREATE,
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
            RESOURCE_READ,
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
            RESOURCE_CREATE,
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
            RESOURCE_READ,
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
            RESOURCE_CREATE,
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
            RESOURCE_CREATE,
            Resource::instance("user", "delete-test"),
            FieldSet::all(),
        );
        let target = ExecutionTarget::new(Resource::instance("user", "delete-test"));
        executor.execute(&ctx, &target, &meta, Some(json!({"name": "Delete Me"}))).unwrap();

        // Delete
        let ctx = create_context(
            RESOURCE_DELETE,
            Resource::instance("user", "delete-test"),
            FieldSet::all(),
        );
        let delete_result = executor.execute(&ctx, &target, &meta, None);
        assert!(delete_result.is_ok());

        // Read after delete should fail
        let ctx = create_context(
            RESOURCE_READ,
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
