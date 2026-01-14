//! SQLite state backend.
//!
//! The **reference implementation** of the State contract.
//! All other backends must match SQLite semantics.

use std::sync::Mutex;

use rusqlite::{Connection, params, OptionalExtension};
use serde_json::{json, Value};

use crate::execution::ExecutionTarget;
use crate::protocol::FieldSet;

use super::capabilities::StateCapabilities;
use super::error::StateError;
use super::traits::State;

/// SQLite state backend.
///
/// This is the **reference implementation** of the State trait.
/// If Postgres disagrees with SQLite, Postgres is wrong.
pub struct SqliteState {
    conn: Mutex<Connection>,
    capabilities: StateCapabilities,
}

impl SqliteState {
    /// Open a SQLite database at the given path.
    pub fn open(path: &str) -> Result<Self, StateError> {
        let conn = Connection::open(path)
            .map_err(|e| StateError::ConnectionError(e.to_string()))?;
        
        let state = Self {
            conn: Mutex::new(conn),
            capabilities: StateCapabilities::sqlite(),
        };
        
        state.initialize_schema()?;
        Ok(state)
    }

    /// Create an in-memory SQLite database (for testing).
    pub fn in_memory() -> Result<Self, StateError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| StateError::ConnectionError(e.to_string()))?;
        
        let state = Self {
            conn: Mutex::new(conn),
            capabilities: StateCapabilities::sqlite(),
        };
        
        state.initialize_schema()?;
        Ok(state)
    }

    /// Initialize the state schema.
    fn initialize_schema(&self) -> Result<(), StateError> {
        let conn = self.conn.lock().unwrap();
        
        // Single table for all resources (document-style)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS resources (
                resource_type TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                data TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                deleted INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                PRIMARY KEY (resource_type, resource_id)
            )",
            [],
        ).map_err(|e| StateError::InternalError(e.to_string()))?;

        Ok(())
    }

    /// Build state key from target.
    fn state_key(target: &ExecutionTarget) -> (String, Option<String>) {
        (
            target.resource.resource_type.clone(),
            target.resource.resource_id.clone(),
        )
    }

    /// Filter fields from data.
    fn filter_fields(data: Value, fields: &FieldSet) -> Value {
        if fields.is_all() {
            return data;
        }

        match data {
            Value::Object(map) => {
                let filtered: serde_json::Map<String, Value> = map
                    .into_iter()
                    .filter(|(key, _)| fields.contains(key))
                    .collect();
                Value::Object(filtered)
            }
            Value::Array(arr) => {
                Value::Array(arr.into_iter().map(|v| Self::filter_fields(v, fields)).collect())
            }
            other => other,
        }
    }

    /// Validate CAS constraints against current state.
    fn validate_constraints(
        constraints: Option<&Value>,
        current_version: Option<i64>,
        current_deleted: Option<bool>,
    ) -> Result<(), StateError> {
        let constraints = match constraints {
            Some(c) => c,
            None => return Ok(()),
        };

        let obj = constraints.as_object().ok_or_else(|| StateError::ConstraintViolation {
            constraint: "format".to_string(),
            reason: "constraints must be an object".to_string(),
        })?;

        for (key, value) in obj {
            match key.as_str() {
                "version_eq" => {
                    let expected = value.as_u64().ok_or_else(|| StateError::ConstraintViolation {
                        constraint: "version_eq".to_string(),
                        reason: "expected version must be a number".to_string(),
                    })?;
                    let actual = current_version.ok_or_else(|| StateError::ConstraintViolation {
                        constraint: "version_eq".to_string(),
                        reason: "resource not found".to_string(),
                    })?;
                    if actual as u64 != expected {
                        return Err(StateError::ConstraintViolation {
                            constraint: "version_eq".to_string(),
                            reason: format!("expected {}, got {}", expected, actual),
                        });
                    }
                }
                "not_deleted" => {
                    if value.as_bool().unwrap_or(false) {
                        let deleted = current_deleted.unwrap_or(false);
                        if deleted {
                            return Err(StateError::ConstraintViolation {
                                constraint: "not_deleted".to_string(),
                                reason: "resource is deleted".to_string(),
                            });
                        }
                    }
                }
                "exists" => {
                    let should_exist = value.as_bool().unwrap_or(true);
                    let exists = current_version.is_some();
                    if should_exist && !exists {
                        return Err(StateError::ConstraintViolation {
                            constraint: "exists".to_string(),
                            reason: "resource must exist".to_string(),
                        });
                    }
                    if !should_exist && exists {
                        return Err(StateError::ConstraintViolation {
                            constraint: "exists".to_string(),
                            reason: "resource must not exist".to_string(),
                        });
                    }
                }
                _ => {} // Ignore unknown constraints (forward compatibility)
            }
        }

        Ok(())
    }

    /// Read internal tables.
    fn read_internal(&self, resource_type: &str, id: &str) -> Result<Value, StateError> {
        let conn = self.conn.lock().unwrap();
        match resource_type {
            "__models" => {
                let mut stmt = conn.prepare("SELECT id, name, namespace, created_at FROM __models WHERE id = ?1")
                    .map_err(|e| StateError::InternalError(e.to_string()))?;
                let mut rows = stmt.query(params![id])
                    .map_err(|e| StateError::InternalError(e.to_string()))?;
                
                if let Some(row) = rows.next().map_err(|e| StateError::InternalError(e.to_string()))? {
                    // Fetch fields
                    let mut fields_stmt = conn.prepare("SELECT id, name, field_type, required, unique_flag, default_val, created_at FROM __fields WHERE model_id = ?1")
                        .map_err(|e| StateError::InternalError(e.to_string()))?;
                    let fields_iter = fields_stmt.query_map(params![id], |row| {
                        let f_type_str: String = row.get(2)?;
                        let default_str: Option<String> = row.get(5)?;
                        
                        Ok(json!({
                            "id": row.get::<_, String>(0)?,
                            "name": row.get::<_, String>(1)?,
                            "field_type": serde_json::from_str::<Value>(&f_type_str).unwrap_or(Value::Null),
                            "required": row.get::<_, i64>(3)? != 0,
                            "unique": row.get::<_, i64>(4)? != 0,
                            "default": default_str.map(|s| serde_json::from_str::<Value>(&s).unwrap_or(Value::Null)),
                            "created_at": row.get::<_, i64>(6)?,
                        }))
                    }).map_err(|e| StateError::InternalError(e.to_string()))?;

                    let fields: Vec<Value> = fields_iter.filter_map(Result::ok).collect();

                    Ok(json!({
                        "id": row.get::<_, String>(0).unwrap(),
                        "name": row.get::<_, String>(1).unwrap(),
                        "namespace": row.get::<_, String>(2).unwrap(),
                        "created_at": row.get::<_, i64>(3).unwrap(),
                        "fields": fields
                    }))
                } else {
                    Err(StateError::NotFound {
                        resource_type: resource_type.to_string(),
                        resource_id: id.to_string(),
                    })
                }
            },
            "__fields" => {
                let mut stmt = conn.prepare("SELECT id, model_id, name, field_type, required, unique_flag, default_val, created_at FROM __fields WHERE id = ?1")
                   .map_err(|e| StateError::InternalError(e.to_string()))?;
                if let Some(row) = stmt.query_row(params![id], |row| {
                    let f_type_str: String = row.get(3)?;
                    let default_str: Option<String> = row.get(6)?;
                    Ok(json!({
                        "id": row.get::<_, String>(0)?,
                        "model_id": row.get::<_, String>(1)?,
                        "name": row.get::<_, String>(2)?,
                        "field_type": serde_json::from_str::<Value>(&f_type_str).unwrap_or(Value::Null),
                        "required": row.get::<_, i64>(4)? != 0,
                        "unique": row.get::<_, i64>(5)? != 0,
                        "default": default_str.map(|s| serde_json::from_str::<Value>(&s).unwrap_or(Value::Null)),
                        "created_at": row.get::<_, i64>(7)?,
                    }))
                }).optional().map_err(|e| StateError::InternalError(e.to_string()))? {
                    Ok(row)
                } else {
                     Err(StateError::NotFound {
                        resource_type: resource_type.to_string(),
                        resource_id: id.to_string(),
                    })
                }
            },
            "__internal_users" => {
                let mut stmt = conn.prepare("SELECT id, external_subject, roles, status, created_at FROM __internal_users WHERE id = ?1")
                    .map_err(|e| StateError::InternalError(e.to_string()))?;
                if let Some(row) = stmt.query_row(params![id], |row| {
                    let roles_str: String = row.get(2)?;
                    Ok(json!({
                        "id": row.get::<_, String>(0)?,
                        "external_subject": row.get::<_, String>(1)?,
                        "roles": serde_json::from_str::<Value>(&roles_str).unwrap_or(json!([])),
                        "status": row.get::<_, String>(3)?,
                        "created_at": row.get::<_, i64>(4)?,
                    }))
                }).optional().map_err(|e| StateError::InternalError(e.to_string()))? {
                    Ok(row)
                } else {
                     Err(StateError::NotFound {
                        resource_type: resource_type.to_string(),
                        resource_id: id.to_string(),
                    })
                }
            },
            _ => Err(StateError::NotFound {
                 resource_type: resource_type.to_string(),
                 resource_id: id.to_string(),
            }),
        }
    }

    fn write_internal(&self, resource_type: &str, id: &str, data: Value) -> Result<u64, StateError> {
        let conn = self.conn.lock().unwrap();
        match resource_type {
            "__models" => {
                let name = data["name"].as_str().ok_or(StateError::BadRequest("missing name".to_string()))?;
                let namespace = data["namespace"].as_str().unwrap_or("public");
                let created_at = data["created_at"].as_i64().unwrap_or(0); // Should be set by caller
                
                conn.execute(
                    "INSERT INTO __models (id, name, namespace, created_at) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(id) DO UPDATE SET name=excluded.name, namespace=excluded.namespace",
                    params![id, name, namespace, created_at],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;
                
                Ok(1)
            },
            "__fields" => {
                let model_id = data["model_id"].as_str().ok_or(StateError::BadRequest("missing model_id".to_string()))?;
                let name = data["name"].as_str().ok_or(StateError::BadRequest("missing name".to_string()))?;
                let f_type = serde_json::to_string(&data["field_type"]).unwrap();
                let required = data["required"].as_bool().unwrap_or(false);
                let unique = data["unique"].as_bool().unwrap_or(false);
                let default_val = if data["default"].is_null() { None } else { Some(serde_json::to_string(&data["default"]).unwrap()) };
                let created_at = data["created_at"].as_i64().unwrap_or(0);

                conn.execute(
                    "INSERT INTO __fields (id, model_id, name, field_type, required, unique_flag, default_val, created_at) 
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(id) DO UPDATE SET name=excluded.name, required=excluded.required, unique_flag=excluded.unique_flag, default_val=excluded.default_val",
                    params![id, model_id, name, f_type, required, unique, default_val, created_at],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;
                Ok(1)
            },
            "__internal_users" => {
                let external_subject = data["external_subject"].as_str().ok_or(StateError::BadRequest("missing external_subject".to_string()))?;
                let roles = serde_json::to_string(&data["roles"]).unwrap();
                let status = data["status"].as_str().unwrap_or("active");
                let created_at = data["created_at"].as_i64().unwrap_or(0);
                
                conn.execute(
                    "INSERT INTO __internal_users (id, external_subject, roles, status, created_at) 
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(id) DO UPDATE SET roles=excluded.roles, status=excluded.status",
                    params![id, external_subject, roles, status, created_at],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;
                Ok(1)
            },
            _ => Err(StateError::BadRequest(format!("cannot write to internal table {}", resource_type))),
        }
    }

    fn delete_internal(&self, resource_type: &str, id: &str) -> Result<u64, StateError> {
        let conn = self.conn.lock().unwrap();
        match resource_type {
            "__fields" => {
                let count = conn.execute(
                    "DELETE FROM __fields WHERE id = ?1",
                    params![id],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;
                Ok(count as u64)
            },
            // Cannot delete models or internal users via this API yet/ever?
            // Models deletion should cascade from __models, but maybe safe to expose.
            "__models" => {
                 let count = conn.execute(
                    "DELETE FROM __models WHERE id = ?1",
                    params![id],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;
                Ok(count as u64)
            },
            _ => Err(StateError::BadRequest(format!("cannot delete from internal table {}", resource_type))),
        }
    }
}

impl State for SqliteState {
    fn read(
        &self,
        target: &ExecutionTarget,
        fields: &FieldSet,
        constraints: Option<&Value>,
    ) -> Result<Value, StateError> {
        let (resource_type, resource_id) = Self::state_key(target);

        match resource_id {
            Some(id) => {
                // Instance read
                if resource_type.starts_with("__") {
                    return self.read_internal(&resource_type, &id);
                }

                let conn = self.conn.lock().unwrap();
                let result: Result<(String, i64, bool), rusqlite::Error> = conn.query_row(
                    "SELECT data, version, deleted FROM resources 
                     WHERE resource_type = ?1 AND resource_id = ?2",
                    params![resource_type, id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? != 0)),
                );

                match result {
                    Ok((data_str, version, deleted)) => {
                        // Soft-deleted records are treated as not found
                        if deleted {
                            return Err(StateError::NotFound {
                                resource_type,
                                resource_id: id,
                            });
                        }

                        Self::validate_constraints(constraints, Some(version), Some(deleted))?;
                        
                        let mut data: Value = serde_json::from_str(&data_str)
                            .map_err(|e| StateError::InternalError(e.to_string()))?;
                        
                        // Inject metadata
                        if let Some(obj) = data.as_object_mut() {
                            obj.insert("_version".to_string(), json!(version));
                        }
                        
                        Ok(Self::filter_fields(data, fields))
                    }
                    Err(rusqlite::Error::QueryReturnedNoRows) => {
                        Err(StateError::NotFound {
                            resource_type,
                            resource_id: id,
                        })
                    }
                    Err(e) => Err(StateError::InternalError(e.to_string())),
                }
            }
            None => {
                let conn = self.conn.lock().unwrap();
                // Collection read
                let mut stmt = conn.prepare(
                    "SELECT resource_id, data, version, deleted FROM resources 
                     WHERE resource_type = ?1 AND deleted = 0"
                ).map_err(|e| StateError::InternalError(e.to_string()))?;

                let rows = stmt.query_map(params![resource_type], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                }).map_err(|e| StateError::InternalError(e.to_string()))?;

                let mut results = Vec::new();
                for row in rows {
                    let (id, data_str, version) = row.map_err(|e| StateError::InternalError(e.to_string()))?;
                    let mut data: Value = serde_json::from_str(&data_str)
                        .map_err(|e| StateError::InternalError(e.to_string()))?;
                    
                    if let Some(obj) = data.as_object_mut() {
                        obj.insert("_id".to_string(), json!(id));
                        obj.insert("_version".to_string(), json!(version));
                    }
                    
                    results.push(Self::filter_fields(data, fields));
                }

                Ok(Value::Array(results))
            }
        }
    }

    fn write(
        &self,
        target: &ExecutionTarget,
        _fields: &FieldSet,
        payload: &Value,
        constraints: Option<&Value>,
    ) -> Result<u64, StateError> {
        let (resource_type, resource_id) = Self::state_key(target);

        let id = resource_id.unwrap_or_else(|| {
            // Generate ID for new resources
            format!("{:016x}", rand::random::<u64>())
        });

        if resource_type.starts_with("__") {
            return self.write_internal(&resource_type, &id, payload.clone());
        }

        let conn = self.conn.lock().unwrap();

        // Check current state for constraints
        let current: Option<(i64, bool)> = conn.query_row(
            "SELECT version, deleted FROM resources 
             WHERE resource_type = ?1 AND resource_id = ?2",
            params![resource_type, id],
            |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
        ).ok();

        let (current_version, current_deleted) = match current {
            Some((v, d)) => (Some(v), Some(d)),
            None => (None, None),
        };

        Self::validate_constraints(constraints, current_version, current_deleted)?;

        let data_str = serde_json::to_string(payload)
            .map_err(|e| StateError::InternalError(e.to_string()))?;

        let affected = if current_version.is_some() {
            // Update
            conn.execute(
                "UPDATE resources SET data = ?1, version = version + 1, 
                 updated_at = strftime('%s', 'now'), deleted = 0
                 WHERE resource_type = ?2 AND resource_id = ?3",
                params![data_str, resource_type, id],
            ).map_err(|e| StateError::InternalError(e.to_string()))?
        } else {
            // Insert
            conn.execute(
                "INSERT INTO resources (resource_type, resource_id, data) 
                 VALUES (?1, ?2, ?3)",
                params![resource_type, id, data_str],
            ).map_err(|e| StateError::InternalError(e.to_string()))?
        };

        Ok(affected as u64)
    }

    fn delete(
        &self,
        target: &ExecutionTarget,
        constraints: Option<&Value>,
    ) -> Result<u64, StateError> {
        let (resource_type, resource_id) = Self::state_key(target);
        if resource_type.starts_with("__") {
            if let Some(id) = resource_id {
                 return self.delete_internal(&resource_type, &id);
            }
             return Err(StateError::BadRequest("collection delete not supported for internal tables".to_string()));
        }

        let conn = self.conn.lock().unwrap();
        match resource_id {
            Some(id) => {
                // Check constraints
                let current: Option<(i64, bool)> = conn.query_row(
                    "SELECT version, deleted FROM resources 
                     WHERE resource_type = ?1 AND resource_id = ?2",
                    params![resource_type, id],
                    |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
                ).ok();

                let (current_version, current_deleted) = match current {
                    Some((v, d)) => (Some(v), Some(d)),
                    None => (None, None),
                };

                Self::validate_constraints(constraints, current_version, current_deleted)?;

                // Soft delete
                let affected = conn.execute(
                    "UPDATE resources SET deleted = 1, updated_at = strftime('%s', 'now')
                     WHERE resource_type = ?1 AND resource_id = ?2 AND deleted = 0",
                    params![resource_type, id],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;

                if affected == 0 && current_version.is_none() {
                    return Err(StateError::NotFound {
                        resource_type,
                        resource_id: id,
                    });
                }

                Ok(affected as u64)
            }
            None => {
                // Collection delete (soft delete all)
                let affected = conn.execute(
                    "UPDATE resources SET deleted = 1, updated_at = strftime('%s', 'now')
                     WHERE resource_type = ?1 AND deleted = 0",
                    params![resource_type],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;

                Ok(affected as u64)
            }
        }
    }

    fn execute_ddl(&self, sql: &str) -> Result<(), StateError> {
        self.conn.lock().unwrap().execute_batch(sql).map_err(|e| StateError::InternalError(e.to_string()))
    }


    fn capabilities(&self) -> &StateCapabilities {
        &self.capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Resource;

    fn create_state() -> SqliteState {
        SqliteState::in_memory().unwrap()
    }

    fn create_target(resource_type: &str, id: Option<&str>) -> ExecutionTarget {
        let resource = match id {
            Some(id) => Resource::instance(resource_type, id),
            None => Resource::collection(resource_type),
        };
        ExecutionTarget::new(resource)
    }

    #[test]
    fn test_write_then_read() {
        let state = create_state();
        let target = create_target("user", Some("123"));

        let payload = json!({"name": "Alice", "email": "alice@example.com"});
        state.write(&target, &FieldSet::all(), &payload, None).unwrap();

        let result = state.read(&target, &FieldSet::all(), None).unwrap();
        assert_eq!(result.get("name"), Some(&json!("Alice")));
        assert_eq!(result.get("email"), Some(&json!("alice@example.com")));
    }

    #[test]
    fn test_read_nonexistent() {
        let state = create_state();
        let target = create_target("user", Some("nonexistent"));

        let result = state.read(&target, &FieldSet::all(), None);
        assert!(matches!(result, Err(StateError::NotFound { .. })));
    }

    #[test]
    fn test_field_filtering() {
        let state = create_state();
        let target = create_target("user", Some("123"));

        let payload = json!({"name": "Alice", "email": "alice@example.com", "password": "secret"});
        state.write(&target, &FieldSet::all(), &payload, None).unwrap();

        let fields = FieldSet::new(["name", "email"]);
        let result = state.read(&target, &fields, None).unwrap();
        
        assert!(result.get("name").is_some());
        assert!(result.get("email").is_some());
        assert!(result.get("password").is_none());
    }

    #[test]
    fn test_version_constraint_success() {
        let state = create_state();
        let target = create_target("user", Some("123"));

        state.write(&target, &FieldSet::all(), &json!({"name": "v1"}), None).unwrap();

        // Update with correct version
        let constraints = json!({"version_eq": 1});
        let result = state.write(&target, &FieldSet::all(), &json!({"name": "v2"}), Some(&constraints));
        assert!(result.is_ok());
    }

    #[test]
    fn test_version_constraint_failure() {
        let state = create_state();
        let target = create_target("user", Some("123"));

        state.write(&target, &FieldSet::all(), &json!({"name": "v1"}), None).unwrap();

        // Update with wrong version
        let constraints = json!({"version_eq": 99});
        let result = state.write(&target, &FieldSet::all(), &json!({"name": "v2"}), Some(&constraints));
        assert!(matches!(result, Err(StateError::ConstraintViolation { .. })));
    }

    #[test]
    fn test_delete_existing() {
        let state = create_state();
        let target = create_target("user", Some("123"));

        state.write(&target, &FieldSet::all(), &json!({"name": "Alice"}), None).unwrap();
        
        let deleted = state.delete(&target, None).unwrap();
        assert_eq!(deleted, 1);

        // Should not be readable after delete
        let result = state.read(&target, &FieldSet::all(), None);
        assert!(matches!(result, Err(StateError::NotFound { .. })));
    }

    #[test]
    fn test_collection_read() {
        let state = create_state();
        
        state.write(&create_target("user", Some("1")), &FieldSet::all(), &json!({"name": "Alice"}), None).unwrap();
        state.write(&create_target("user", Some("2")), &FieldSet::all(), &json!({"name": "Bob"}), None).unwrap();

        let collection_target = create_target("user", None);
        let result = state.read(&collection_target, &FieldSet::all(), None).unwrap();
        
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_capabilities() {
        let state = create_state();
        let caps = state.capabilities();
        
        assert!(caps.transactions);
        assert!(caps.cas_constraints);
        assert_eq!(caps.backend_name, "sqlite");
    }
}
