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
use crate::schema::validation::{validate_identifier, quote_identifier};
use uuid::Uuid;

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

    /// Execute a function with access to the underlying connection.
    /// 
    /// This is used by the Identity Provider for auth-specific queries
    /// that don't fit the standard State trait interface.
    pub fn with_connection<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&Connection) -> T,
    {
        let conn = self.conn.lock().unwrap();
        f(&conn)
    }

    // ========================================================================
    // Health Check Methods
    // ========================================================================

    /// Check if the database connection is alive.
    /// 
    /// This is used by the health endpoint to detect degraded states.
    /// Never panics — returns Err on any DB issue.
    pub fn ping(&self) -> Result<(), StateError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("SELECT 1")
            .map_err(|e| StateError::ConnectionError(e.to_string()))
    }

    /// Check if any internal user exists (bootstrap complete).
    /// 
    /// Returns Ok(true) if at least one internal user exists.
    /// Returns Ok(false) if the table is empty.
    /// Returns Err if DB query fails.
    pub fn has_any_internal_user(&self) -> Result<bool, StateError> {
        let conn = self.conn.lock().unwrap();
        
        // First check if table exists
        let table_exists: bool = conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='__internal_users'",
            [],
            |_| Ok(true)
        ).unwrap_or(false);

        if !table_exists {
            return Ok(false);
        }

        // Check if any user exists
        let has_user: bool = conn.query_row(
            "SELECT 1 FROM __internal_users LIMIT 1",
            [],
            |_| Ok(true)
        ).unwrap_or(false);

        Ok(has_user)
    }

    /// Check if any admin user exists.
    /// 
    /// Returns Ok(true) if at least one user with 'admin' role exists.
    pub fn has_admin_user(&self) -> Result<bool, StateError> {
        let conn = self.conn.lock().unwrap();
        
        // First check if table exists
        let table_exists: bool = conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='__internal_users'",
            [],
            |_| Ok(true)
        ).unwrap_or(false);

        if !table_exists {
            return Ok(false);
        }

        // Check if any admin exists using json_each
        let has_admin: bool = conn.query_row(
            "SELECT 1 FROM __internal_users, json_each(roles) WHERE json_each.value = 'admin' LIMIT 1",
            [],
            |_| Ok(true)
        ).unwrap_or(false);

        Ok(has_admin)
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
    fn read_internal(&self, resource_type: &str, id: Option<&str>, constraints: Option<&Value>) -> Result<Value, StateError> {
        let conn = self.conn.lock().unwrap();
        match resource_type {
            "__models" => self.read_internal_models(&conn, id, constraints),
            "__fields" => {
                if let Some(resource_id) = id {
                    let mut stmt = conn.prepare("SELECT id, model_id, name, field_type, required, unique_flag, default_val, created_at FROM __fields WHERE id = ?1")
                       .map_err(|e| StateError::InternalError(e.to_string()))?;
                    if let Some(row) = stmt.query_row(params![resource_id], |row| {
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
                            resource_id: resource_id.to_string(),
                        })
                    }
                } else {
                    // Collection Read
                    let mut sql = "SELECT id, model_id, name, field_type, required, unique_flag, default_val, created_at FROM __fields".to_string();
                    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

                    if let Some(c) = constraints {
                        if let Some(name_filter) = c.get("name").and_then(|v| v.as_str()) {
                            sql.push_str(" WHERE name = ?1");
                            params_vec.push(Box::new(name_filter.to_string()));
                        }
                    }

                    let mut stmt = conn.prepare(&sql).map_err(|e| StateError::InternalError(e.to_string()))?;
                    let fields_iter = stmt.query_map(rusqlite::params_from_iter(params_vec.iter()), |row| {
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
                    }).map_err(|e| StateError::InternalError(e.to_string()))?;

                    let fields: Vec<Value> = fields_iter.filter_map(Result::ok).collect();
                    Ok(json!(fields))
                }
            },
            "__internal_users" => {
                let id = id.ok_or(StateError::BadRequest("collection read not supported for __internal_users".into()))?;
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
            "__object_namespaces" => {
                let id = id.ok_or(StateError::BadRequest("collection read not supported for __object_namespaces".into()))?;
                let mut stmt = conn.prepare("SELECT id, name, owner_id, backend, root_path, created_at FROM __object_namespaces WHERE id = ?1")
                    .map_err(|e| StateError::InternalError(e.to_string()))?;
                
                if let Some(row) = stmt.query_row(params![id], |row| {
                    Ok(json!({
                        "id": row.get::<_, String>(0)?,
                        "name": row.get::<_, String>(1)?,
                        "owner_id": row.get::<_, Option<String>>(2)?,
                        "backend": row.get::<_, String>(3)?,
                        "root_path": row.get::<_, String>(4)?,
                        "created_at": row.get::<_, i64>(5)?,
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
                 resource_id: id.unwrap_or("collection").to_string(),
            }),
        }
    }

    fn read_internal_models(&self, conn: &rusqlite::Connection, id: Option<&str>, constraints: Option<&Value>) -> Result<Value, StateError> {
        if let Some(model_id) = id {
            // Instance Read
             let mut stmt = conn.prepare("SELECT id, name, namespace, created_at, owner_id FROM __models WHERE id = ?1")
                    .map_err(|e| StateError::InternalError(e.to_string()))?;
                let mut rows = stmt.query(params![model_id])
                    .map_err(|e| StateError::InternalError(e.to_string()))?;
                
                if let Some(row) = rows.next().map_err(|e| StateError::InternalError(e.to_string()))? {
                    // Check ownership constraint if present?
                    let owner: Option<String> = row.get(4).unwrap_or(None);
                    // Standard constraint checking mechanism usually happens via SQL.
                    // For now, we manually check if "owner_id" constraint is present.
                    if let Some(c) = constraints {
                        if let Some(req_owner) = c.get("owner_id").and_then(|v| v.as_str()) {
                             // System models (None owner) are visible to all? Or strict?
                             // Strict: You see only what you own. None owner = System.
                             // If I request my models, I shouldn't see system models?
                             // Or should I see system models too?
                             // "You can only see models YOU own"
                             if let Some(actual) = &owner {
                                 if actual != req_owner {
                                     return Err(StateError::NotFound { resource_type: "__models".into(), resource_id: model_id.into() });
                                 }
                             } else {
                                 // System model. Allow read? 
                                 // "Admin bypass" usually handled by not passing constraint?
                                 // Or by policy. If constraint is passed, it implies filtering.
                                 // Let's assume strict filtering: If constraint is owner_id=X, then only models owned by X.
                                 return Err(StateError::NotFound { resource_type: "__models".into(), resource_id: model_id.into() });
                             }
                        }
                    }

                    // Fetch fields
                    let mut fields_stmt = conn.prepare("SELECT id, name, field_type, required, unique_flag, default_val, created_at FROM __fields WHERE model_id = ?1")
                        .map_err(|e| StateError::InternalError(e.to_string()))?;
                    let fields_iter = fields_stmt.query_map(params![model_id], |row| {
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
                        "owner_id": owner.unwrap_or_else(|| "system".to_string()),
                        "fields": fields
                    }))
                } else {
                    Err(StateError::NotFound {
                        resource_type: "__models".to_string(),
                        resource_id: model_id.to_string(),
                    })
                }
        } else {
            // Collection Read
            let mut sql = "SELECT id, name, namespace, created_at, owner_id FROM __models".to_string();
            let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(c) = constraints {
                if let Some(req_owner) = c.get("owner_id").and_then(|v| v.as_str()) {
                    sql.push_str(" WHERE owner_id = ?1");
                    params_vec.push(Box::new(req_owner.to_string()));
                }
            }
            
            let mut stmt = conn.prepare(&sql).map_err(|e| StateError::InternalError(e.to_string()))?;
            
            // We need to convert to params slice.
            // Since we have max 1 param, let's simplify.
            
            let map_fn = |row: &rusqlite::Row| -> rusqlite::Result<(String, String, String, i64, Option<String>)> {
                 Ok((
                    row.get(0)?, 
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?
                ))
            };

            let rows_result = if params_vec.is_empty() {
                stmt.query_map([], map_fn)
            } else {
                 stmt.query_map(params![params_vec[0]], map_fn)
            };

            let rows = rows_result.map_err(|e| StateError::InternalError(e.to_string()))?;
            
            let mut models = Vec::new();
            for r in rows {
                if let Ok((id, name, namespace, created_at, owner)) = r {
                    models.push(json!({
                        "id": id,
                        "name": name,
                        "namespace": namespace,
                        "created_at": created_at,
                        "owner_id": owner.unwrap_or_else(|| "system".to_string()),
                    }));
                }
            }
            
            Ok(json!(models)) 
        }
    }

    fn write_internal(&self, resource_type: &str, id: &str, data: Value) -> Result<u64, StateError> {
        let conn = self.conn.lock().unwrap();
        match resource_type {
            "__models" => {
                let name = data["name"].as_str().ok_or(StateError::BadRequest("missing name".to_string()))?;
                let namespace = data["namespace"].as_str().unwrap_or("public");
                let created_at = data["created_at"].as_i64().unwrap_or(0); // Should be set by caller
                let owner_id = data["owner_id"].as_str(); // Optional owner

                conn.execute(
                    "INSERT INTO __models (id, name, namespace, created_at, owner_id) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(id) DO UPDATE SET name=excluded.name, namespace=excluded.namespace, owner_id=excluded.owner_id",
                    params![id, name, namespace, created_at, owner_id],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;
                
                // Create physical table
                // Drop lock before calling helper helper (it acquires lock itself)
                drop(conn);
                self.create_physical_table(name)?;
                
                // 3. Register system fields in __fields metadata
                // This ensures they are visible to the execution layer and introspection
                // We use resource.create internally (or direct SQL) to avoid alter_add_column loop
                let system_fields = vec![
                    ("id", "String", true),
                    ("owner_id", "String", true), // Hardened: now required/present
                    ("created_at", "Int", true),
                    ("updated_at", "Int", true),
                ];

                let conn = self.conn.lock().unwrap();
                for (f_name, f_type, required) in system_fields {
                    let f_id = format!("{}-{}", id, f_name);
                    let f_type_json = serde_json::json!({ "type": f_type });
                    let f_type_str = serde_json::to_string(&f_type_json).unwrap();
                    
                    let _ = conn.execute(
                        "INSERT INTO __fields (id, model_id, name, field_type, required, unique_flag, owner_id, created_at) 
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                         ON CONFLICT(id) DO NOTHING",
                        params![f_id, id, f_name, f_type_str, required, f_name == "id", owner_id, created_at],
                    );
                }
                
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
                
                // Fetch model name needed for physical table
                let model_name: String = conn.query_row(
                    "SELECT name FROM __models WHERE id = ?1",
                    params![model_id],
                    |row| row.get(0),
                ).map_err(|_| StateError::BadRequest(format!("model {} not found", model_id)))?;

                conn.execute(
                    "INSERT INTO __fields (id, model_id, name, field_type, required, unique_flag, default_val, created_at) 
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(id) DO UPDATE SET name=excluded.name, required=excluded.required, unique_flag=excluded.unique_flag, default_val=excluded.default_val",
                    params![id, model_id, name, f_type, required, unique, default_val, created_at],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;

                // Add col to physical table
                drop(conn);
                self.alter_add_column(&model_name, name, &data)?;
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
            "__object_namespaces" => {
                let name = data["name"].as_str().ok_or(StateError::BadRequest("missing name".to_string()))?;
                let backend = data["backend"].as_str().ok_or(StateError::BadRequest("missing backend".to_string()))?;
                let root_path = data["root_path"].as_str().ok_or(StateError::BadRequest("missing root_path".to_string()))?;
                let created_at = data["created_at"].as_i64().unwrap_or(0);
                let owner_id = data["owner_id"].as_str();

                conn.execute(
                    "INSERT INTO __object_namespaces (id, name, owner_id, backend, root_path, created_at) 
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET name=excluded.name, owner_id=excluded.owner_id, backend=excluded.backend, root_path=excluded.root_path",
                    params![id, name, owner_id, backend, root_path, created_at],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;
                Ok(1)
            },
            _ => Err(StateError::BadRequest(format!("cannot write to internal table {}", resource_type))),
        }
    }

    /// Create a physical table for a model.
    fn create_physical_table(&self, model_name: &str) -> Result<(), StateError> {
        validate_identifier(model_name)
            .map_err(|e| StateError::BadRequest(e))?;
        
        let table_name = quote_identifier(model_name);
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            ) STRICT;", 
            table_name
        );

        self.conn.lock().unwrap().execute(&sql, [])
            .map_err(|e| StateError::InternalError(e.to_string()))?;
        
        Ok(())
    }

    /// Add a column to a physical table.
    fn alter_add_column(&self, model_name: &str, field_name: &str, field_data: &Value) -> Result<(), StateError> {
        validate_identifier(model_name).map_err(StateError::BadRequest)?;
        validate_identifier(field_name).map_err(StateError::BadRequest)?;

        let table_name = quote_identifier(model_name);
        let column_name = quote_identifier(field_name);
        
        // Map FieldType to SQLite Type
        let field_type = &field_data["field_type"];
        let type_name = field_type["type"].as_str().unwrap_or("String");
        
        let sql_type = match type_name {
            "String" | "Json" | "Ref" => "TEXT",
            "Int" | "Bool" => "INTEGER",
            "Float" => "REAL",
            _ => "TEXT",
        };

        // SQLite ADD COLUMN limitations: cannot add NOT NULL without DEFAULT
        // For now, we add as NULLABLE unless default is provided.
        // STRICT tables enforce types, but NULLability is separate.
        
        let sql = format!(
            "ALTER TABLE {} ADD COLUMN {} {}",
            table_name, column_name, sql_type
        );

        self.conn.lock().unwrap().execute(&sql, [])
            .map_err(|e| StateError::InternalError(e.to_string()))?;
            
        Ok(())
    }

    /// Drop a column from a physical table.
    fn alter_drop_column(&self, model_name: &str, field_name: &str) -> Result<(), StateError> {
        validate_identifier(model_name).map_err(StateError::BadRequest)?;
        validate_identifier(field_name).map_err(StateError::BadRequest)?;

        let table_name = quote_identifier(model_name);
        let column_name = quote_identifier(field_name);

        let sql = format!(
            "ALTER TABLE {} DROP COLUMN {}",
            table_name, column_name
        );

        self.conn.lock().unwrap().execute(&sql, [])
            .map_err(|e| StateError::InternalError(e.to_string()))?;
            
        Ok(())
    }

    fn delete_internal(&self, resource_type: &str, id: &str) -> Result<u64, StateError> {
        let conn = self.conn.lock().unwrap();
        match resource_type {
            "__fields" => {
                // Get model name and field name before deleting
                let (model_id, name): (String, String) = conn.query_row(
                    "SELECT model_id, name FROM __fields WHERE id = ?1",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                ).map_err(|_| StateError::BadRequest(format!("field {} not found", id)))?;

                let model_name: String = conn.query_row(
                    "SELECT name FROM __models WHERE id = ?1",
                    params![model_id],
                    |row| row.get(0),
                ).map_err(|_| StateError::BadRequest(format!("model {} not found", model_id)))?;

                let count = conn.execute(
                    "DELETE FROM __fields WHERE id = ?1",
                    params![id],
                ).map_err(|e| StateError::InternalError(e.to_string()))?;

                // Drop col from physical table
                drop(conn);
                self.alter_drop_column(&model_name, &name)?;

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
                    return self.read_internal(&resource_type, Some(&id), constraints);
                }

                let conn = self.conn.lock().unwrap();
                
                // CHECK IF MODEL EXISTS (Physical Table)
                let is_model_defined: bool = conn.query_row(
                    "SELECT 1 FROM __models WHERE name = ?1",
                    params![resource_type],
                    |_| Ok(true),
                ).unwrap_or(false);

                if is_model_defined {
                     let table_name = quote_identifier(&resource_type);
                     // Select all columns? Or filter? 
                     // Select * is easiest, then filter in memory.
                     // TODO: Optimize to select specific fields from SQL.
                     let sql = format!("SELECT * FROM {} WHERE id = ?1", table_name);
                     
                     // We need column names to reconstruct JSON.
                     let mut stmt = conn.prepare(&sql).map_err(|e| StateError::InternalError(e.to_string()))?;
                     let col_names: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
                     
                     let result = stmt.query_row(params![id], |row| {
                         let mut map = serde_json::Map::new();
                         for (i, col_name) in col_names.iter().enumerate() {
                             let val_ref = row.get_ref(i)?;
                             let val = match val_ref {
                                 rusqlite::types::ValueRef::Null => Value::Null,
                                 rusqlite::types::ValueRef::Integer(i) => json!(i),
                                 rusqlite::types::ValueRef::Real(r) => json!(r),
                                 rusqlite::types::ValueRef::Text(t) => {
                                     let s = std::str::from_utf8(t).unwrap_or("");
                                     // Try parsing as JSON if it looks like it? No, explicit schema would be better.
                                     // For now, treat as String.
                                      json!(s)
                                 },
                                 rusqlite::types::ValueRef::Blob(_) => json!("<blob>"), 
                             };
                             map.insert(col_name.clone(), val);
                         }
                         Ok(Value::Object(map))
                     }).optional().map_err(|e| StateError::InternalError(e.to_string()))?;

                    if let Some(mut data) = result {
                        // Inject _version mock? Physical tables don't have version yet unless we added it.
                        // We didn't add version col in create_physical_table.
                        // Todo: Add version col to standard schema.
                        data.as_object_mut().unwrap().insert("_version".to_string(), json!(1));
                        return Ok(Self::filter_fields(data, fields));
                    } else {
                         return Err(StateError::NotFound {
                            resource_type: resource_type.to_string(),
                            resource_id: id.to_string(),
                        });
                    }
                }

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
                if resource_type.starts_with("__") {
                     return self.read_internal(&resource_type, None, constraints);
                }

                let conn = self.conn.lock().unwrap();
                
                // 1. Determine Source (Physical vs Resources)
                let is_model_defined: bool = conn.query_row(
                    "SELECT 1 FROM __models WHERE name = ?1",
                    params![resource_type],
                    |_| Ok(true),
                ).unwrap_or(false);

                let mut results: Vec<Value> = if is_model_defined {
                     let table_name = quote_identifier(&resource_type);
                     let sql = format!("SELECT * FROM {}", table_name);
                     
                     let mut stmt = conn.prepare(&sql).map_err(|e| StateError::InternalError(e.to_string()))?;
                     let col_names: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
                     
                     let rows = stmt.query_map([], |row| {
                         let mut map = serde_json::Map::new();
                         for (i, col_name) in col_names.iter().enumerate() {
                             let val_ref = row.get_ref(i)?;
                             // Simple mapping (same as instance read)
                             let val = match val_ref {
                                 rusqlite::types::ValueRef::Null => Value::Null,
                                 rusqlite::types::ValueRef::Integer(i) => json!(i),
                                 rusqlite::types::ValueRef::Real(r) => json!(r),
                                 rusqlite::types::ValueRef::Text(t) => {
                                     let s = std::str::from_utf8(t).unwrap_or("");
                                      json!(s)
                                 },
                                 rusqlite::types::ValueRef::Blob(_) => json!("<blob>"), 
                             };
                             map.insert(col_name.clone(), val);
                         }
                         Ok(Value::Object(map))
                     }).map_err(|e| StateError::InternalError(e.to_string()))?;

                     rows.filter_map(Result::ok).collect()
                } else {
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

                    let mut items = Vec::new();
                    for row in rows {
                        if let Ok((id, data_str, version)) = row {
                            if let Ok(mut data) = serde_json::from_str::<Value>(&data_str) {
                                if let Some(obj) = data.as_object_mut() {
                                    obj.insert("_id".to_string(), json!(id));
                                    obj.insert("_version".to_string(), json!(version));
                                }
                                items.push(data);
                            }
                        }
                    }
                    items
                };

                // 2. In-Memory Filtering
                if let Some(c) = constraints {
                    if let Some(obj) = c.as_object() {
                        results.retain(|item| {
                            for (k, v) in obj {
                                // Specific CAS handlers
                                if k == "version_eq" || k == "not_deleted" || k == "exists" { continue; }
                                
                                // Direct match
                                if item.get(k) != Some(v) {
                                    return false;
                                }
                            }
                            true
                        });
                    }
                }

                // 3. Field Filtering
                let filtered: Vec<Value> = results.into_iter()
                    .map(|v| Self::filter_fields(v, fields))
                    .collect();

                Ok(Value::Array(filtered))
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

        if resource_type.starts_with("__") {
            return self.write_internal(&resource_type, &id, payload.clone());
        }

        let conn = self.conn.lock().unwrap();
        
        // CHECK IF MODEL EXISTS (Physical Table)
        let is_model_defined: bool = conn.query_row(
            "SELECT 1 FROM __models WHERE name = ?1",
            params![resource_type],
            |_| Ok(true),
        ).unwrap_or(false);

        if is_model_defined {
            // PHYSICAL TABLE WRITE
            let table_name = quote_identifier(&resource_type);
            
            // Should properly map fields to columns. 
            // For now, iterate payload keys. 
            // NOTE: This assumes payload keys match column names.
            // Safety: identifiers are validated at schema creation, ensuring they are safe column names.
            match payload {
                Value::Object(map) => {
                     let mut cols = vec!["id".to_string(), "created_at".to_string(), "updated_at".to_string()];
                     let mut placeholders = vec!["?1".to_string(), "?2".to_string(), "?3".to_string()];
                     let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
                         Box::new(id.clone()),
                         Box::new(0_i64), // created_at placeholder (updated in query)
                         Box::new(0_i64)  // updated_at placeholder
                     ];
                     let mut updates = vec![
                         "updated_at = strftime('%s', 'now')".to_string()
                     ];
                     
                     for (k, v) in map {
                         cols.push(quote_identifier(k));
                         placeholders.push(format!("?{}", values.len() + 1));
                         
                         // Simple conversion for now
                         match v {
                             Value::String(s) => values.push(Box::new(s.clone())),
                             Value::Number(n) => {
                                 if let Some(i) = n.as_i64() {
                                     values.push(Box::new(i));
                                 } else if let Some(f) = n.as_f64() {
                                     values.push(Box::new(f));
                                 } else {
                                     values.push(Box::new(n.to_string()));
                                 }
                             },
                             Value::Bool(b) => values.push(Box::new(if *b { 1 } else { 0 })),
                             _ => values.push(Box::new(v.to_string())), // Json/Array -> Text
                         }
                         
                         updates.push(format!("{} = excluded.{}", quote_identifier(k), quote_identifier(k)));
                     }

                     let _sql = format!(
                         "INSERT INTO {} ({}) VALUES ({})
                          ON CONFLICT(id) DO UPDATE SET {}",
                         table_name,
                         cols.join(", "),
                         placeholders.join(", "),
                         updates.join(", ")
                     );
                     
                     // We need to inject created_at/updated_at logic better or rely on defaults?
                     // For upsert: created_at should be consistent.
                     // Let's refine the VALUES:
                     // created_at = COALESCE((SELECT created_at FROM {table} WHERE id=?1), strftime('%s','now'))
                     // updated_at = strftime('%s','now')
                     // This is hard with single INSERT statement.
                     // Simplification: Always write created_at as now, relies on IGNORE/UPDATE?
                     // ON CONFLICT DO UPDATE SET created_at = created_at (keep old)
                     
                     // Revised SQL construction:
                     // We use named params or positional? Positional is safer but hard to construct dynamically in loop.
                     // rusqlite doesn't support Vec<Box<dyn ToSql>> well directly in params!.
                     // We need to construct a rusqlite::Params object dynamically.
                     // Workaround: Use a loop to bind? No.
                     // Best way: Use `rusqlite::params_from_iter`.
                     
                     // Correct Logic:
                     // 1. Try Update
                     // 2. If 0 rows, Insert
                     
                     // Let's stick to standard INSERT OR REPLACE / UPSERT logic but handle created_at.
                     // The DO UPDATE clause for created_at is `created_at = created_at`.
                     
                     // RE-DOING construction for correctness with `params_from_iter`.
                     // Handle timestamps in Rust
                     let now = std::time::SystemTime::now()
                         .duration_since(std::time::UNIX_EPOCH)
                         .unwrap_or_default()
                         .as_secs() as i64;

                     // 1. Try UPDATE first. 
                     // This handles partial updates without requiring all NOT NULL columns.
                     let mut update_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                     let mut update_assignments = Vec::new();
                     let mut u_idx = 1;

                     // updated_at always changes
                     update_assignments.push(format!("updated_at = ?{}", u_idx));
                     update_values.push(Box::new(now));
                     u_idx += 1;

                     for (k, v) in map {
                         if k == "id" || k == "created_at" || k == "updated_at" { continue; }
                         
                         update_assignments.push(format!("{} = ?{}", quote_identifier(k), u_idx));
                         match v {
                             Value::String(s) => update_values.push(Box::new(s.clone())),
                             Value::Number(n) => {
                                 if let Some(i) = n.as_i64() { update_values.push(Box::new(i)); }
                                 else if let Some(f) = n.as_f64() { update_values.push(Box::new(f)); }
                                 else { update_values.push(Box::new(n.to_string())); }
                             },
                             Value::Bool(b) => update_values.push(Box::new(if *b { 1 } else { 0 })),
                             Value::Null => update_values.push(Box::new(rusqlite::types::Null)),
                             _ => update_values.push(Box::new(v.to_string())),
                         }
                         u_idx += 1;
                     }

                     update_values.push(Box::new(id.clone()));
                     let update_sql = format!(
                         "UPDATE {} SET {} WHERE id = ?{}",
                         table_name,
                         update_assignments.join(", "),
                         u_idx
                     );

                     let affected = conn.execute(&update_sql, rusqlite::params_from_iter(update_values.iter()))
                         .map_err(|e| StateError::InternalError(e.to_string()))?;

                     if affected > 0 {
                         return Ok(affected as u64);
                     }

                     // 2. If no rows affected, try INSERT.
                     // This requires all NOT NULL columns.
                     let mut insert_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                     insert_values.push(Box::new(id.clone())); // ?1
                     
                     let mut col_names = vec!["id".to_string(), "created_at".to_string(), "updated_at".to_string()];
                     let mut placeholders = vec!["?1".to_string(), "?2".to_string(), "?3".to_string()];
                     insert_values.push(Box::new(now)); // ?2
                     insert_values.push(Box::new(now)); // ?3

                     let mut i_idx = 4;
                     for (k, v) in map {
                         if k == "id" || k == "created_at" || k == "updated_at" { continue; }
                         col_names.push(quote_identifier(k));
                         placeholders.push(format!("?{}", i_idx));
                         match v {
                             Value::String(s) => insert_values.push(Box::new(s.clone())),
                             Value::Number(n) => {
                                 if let Some(i) = n.as_i64() { insert_values.push(Box::new(i)); }
                                 else if let Some(f) = n.as_f64() { insert_values.push(Box::new(f)); }
                                 else { insert_values.push(Box::new(n.to_string())); }
                             },
                             Value::Bool(b) => insert_values.push(Box::new(if *b { 1 } else { 0 })),
                             Value::Null => insert_values.push(Box::new(rusqlite::types::Null)),
                             _ => insert_values.push(Box::new(v.to_string())),
                         }
                         i_idx += 1;
                     }

                     let insert_sql = format!(
                         "INSERT INTO {} ({}) VALUES ({})",
                         table_name,
                         col_names.join(", "),
                         placeholders.join(", ")
                     );

                     conn.execute(&insert_sql, rusqlite::params_from_iter(insert_values.iter()))
                         .map_err(|e| StateError::InternalError(e.to_string()))?;
                         
                     return Ok(1);
                },
                _ => return Err(StateError::BadRequest("payload must be an object".to_string())),
            }

        }

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
        
        // CHECK IF MODEL EXISTS (Physical Table)
        let is_model_defined: bool = conn.query_row(
            "SELECT 1 FROM __models WHERE name = ?1",
            params![resource_type],
            |_| Ok(true),
        ).unwrap_or(false);

        if is_model_defined {
             let table_name = quote_identifier(&resource_type);
             
             match resource_id {
                 Some(id) => {
                     // HARD DELETE for physical tables? Or Soft?
                     // Standard dictates soft delete if "deleted" column exists.
                     // But we didn't add "deleted" column in create_physical_table.
                     // Let's do HARD DELETE for now as per MVP strictly typed.
                     // Or should we have added "deleted" col?
                     // Plan said: id, created_at, updated_at. No deleted.
                     // So HARD DELETE.
                     
                     let sql = format!("DELETE FROM {} WHERE id = ?1", table_name);
                     let count = conn.execute(&sql, params![id])
                        .map_err(|e| StateError::InternalError(e.to_string()))?;
                        
                     if count == 0 {
                          return Err(StateError::NotFound {
                            resource_type: resource_type.to_string(),
                            resource_id: id.to_string(),
                        });
                     }
                     return Ok(count as u64);
                 },
                 None => {
                     // Collection delete -> Delete all? Dangerous but consistent.
                     let sql = format!("DELETE FROM {}", table_name);
                     let count = conn.execute(&sql, [])
                        .map_err(|e| StateError::InternalError(e.to_string()))?;
                     return Ok(count as u64);
                 }
             }
        }
        
        // Fallback to resources table using existing logic (moved inside match)
        // We need to re-structure slightly or use the resource_id match.
        
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

    fn ensure_internal_user(&self, external_subject: &str, roles: &[String]) -> Result<String, StateError> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let roles_json = serde_json::to_string(roles).unwrap_or_else(|_| "[]".to_string());

        // Upsert user: Insert if new, update roles if exists.
        // We use returning ID to get the stable UUID.
        
        // 1. Try to find existing
        let existing_id: Option<String> = conn.query_row(
            "SELECT id FROM __internal_users WHERE external_subject = ?1",
            params![external_subject],
            |row| row.get(0),
        ).optional().map_err(|e| StateError::InternalError(e.to_string()))?;

        if let Some(id) = existing_id {
            // Update roles and timestamp? Maybe just roles.
            conn.execute(
                "UPDATE __internal_users SET roles = ?1 WHERE id = ?2",
                params![roles_json, id],
            ).map_err(|e| StateError::InternalError(e.to_string()))?;
            Ok(id)
        } else {
            // Create new
            let new_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO __internal_users (id, external_subject, roles, status, created_at) VALUES (?1, ?2, ?3, 'active', ?4)",
                params![new_id, external_subject, roles_json, now],
            ).map_err(|e| StateError::InternalError(e.to_string()))?;
            Ok(new_id)
        }
    }

    fn count(
        &self,
        target: &ExecutionTarget,
        constraints: Option<&Value>,
    ) -> Result<u64, StateError> {
        let (resource_type, _) = Self::state_key(target);

        // Internal tables count
        if resource_type.starts_with("__") {
             let conn = self.conn.lock().unwrap();
             // Safe because internal tables are known safe
             let count: u64 = conn.query_row(
                 &format!("SELECT COUNT(*) FROM {}", resource_type),
                 [],
                 |row| row.get(0)
             ).map_err(|e| StateError::InternalError(e.to_string()))?;
             return Ok(count);
        }

        let conn = self.conn.lock().unwrap();
        
        // CHECK IF MODEL EXISTS (Physical Table)
        let is_model_defined: bool = conn.query_row(
            "SELECT 1 FROM __models WHERE name = ?1",
            params![resource_type],
            |_| Ok(true),
        ).unwrap_or(false);

        if is_model_defined {
             let table_name = quote_identifier(&resource_type);
             let sql = format!("SELECT COUNT(*) FROM {}", table_name);
             
             let mut where_clauses = Vec::new();
             let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
             
             if let Some(c) = constraints {
                 if let Some(owner) = c.get("owner_id").and_then(|v| v.as_str()) {
                     where_clauses.push("owner_id = ?");
                     params_vec.push(Box::new(owner.to_string()));
                 }
             }

             let final_sql = if where_clauses.is_empty() {
                 sql
             } else {
                 format!("{} WHERE {}", sql, where_clauses.join(" AND "))
             };

             let mut stmt = conn.prepare(&final_sql).map_err(|e| StateError::InternalError(e.to_string()))?;
             
             let count: u64 = if params_vec.is_empty() {
                 stmt.query_row([], |row| row.get(0))
             } else {
                 stmt.query_row(rusqlite::params_from_iter(params_vec.iter()), |row| row.get(0))
             }.map_err(|e| StateError::InternalError(e.to_string()))?;

             return Ok(count);
        }
        
        // Fallback to resources table (only non-deleted)
        let sql = "SELECT COUNT(*) FROM resources WHERE resource_type = ?1 AND deleted = 0";
        let count: u64 = conn.query_row(
            sql,
            params![resource_type],
            |row| row.get(0)
        ).map_err(|e| StateError::InternalError(e.to_string()))?;
        
        Ok(count)
    }

    fn rename_table(&self, old_name: &str, new_name: &str) -> Result<(), StateError> {
        let conn = self.conn.lock().unwrap();
        
        // validate names
        validate_identifier(old_name).map_err(StateError::BadRequest)?;
        validate_identifier(new_name).map_err(StateError::BadRequest)?;

        // 1. Rename in __models
        let changed = conn.execute(
            "UPDATE __models SET name = ?1 WHERE name = ?2",
            params![new_name, old_name]
        ).map_err(|e| StateError::InternalError(e.to_string()))?;

        if changed == 0 {
            return Err(StateError::NotFound { 
                resource_type: "__models".into(), 
                resource_id: old_name.into() 
            });
        }

        // 2. Rename physical table
        let old_tbl = quote_identifier(old_name);
        let new_tbl = quote_identifier(new_name);
        
        conn.execute(
            &format!("ALTER TABLE {} RENAME TO {}", old_tbl, new_tbl),
            []
        ).map_err(|e| StateError::InternalError(e.to_string()))?;

        Ok(())
    }

    fn rename_column(&self, table: &str, old_col: &str, new_col: &str) -> Result<(), StateError> {
        let conn = self.conn.lock().unwrap();

        validate_identifier(table).map_err(StateError::BadRequest)?;
        validate_identifier(old_col).map_err(StateError::BadRequest)?;
        validate_identifier(new_col).map_err(StateError::BadRequest)?;

        // 1. Get model_id
        let model_id: String = conn.query_row(
            "SELECT id FROM __models WHERE name = ?1",
            params![table],
            |row| row.get(0)
        ).map_err(|_| StateError::BadRequest(format!("Model {} not found", table)))?;

        // 2. Rename in __fields
        let changed = conn.execute(
            "UPDATE __fields SET name = ?1 WHERE model_id = ?2 AND name = ?3",
            params![new_col, model_id, old_col]
        ).map_err(|e| StateError::InternalError(e.to_string()))?;

        if changed == 0 {
             // Check if it's a system field (id, created_at, etc)?
             // Or just return NotFound
            return Err(StateError::NotFound { 
                resource_type: "__fields".into(), 
                resource_id: old_col.into() 
            });
        }

        // 3. Rename physical column
        let tbl = quote_identifier(table);
        let old_c = quote_identifier(old_col);
        let new_c = quote_identifier(new_col);

        conn.execute(
            &format!("ALTER TABLE {} RENAME COLUMN {} TO {}", tbl, old_c, new_c),
            []
        ).map_err(|e| StateError::InternalError(e.to_string()))?;

        Ok(())
    }

    fn get_resource_owner(&self, resource_type: &str, resource_id: &str) -> Result<Option<String>, StateError> {
        let conn = self.conn.lock().unwrap();
        
        // 1. Check if model exists/is physical (via __models)
        let is_model_defined: bool = conn.query_row(
            "SELECT 1 FROM __models WHERE name = ?1",
            params![resource_type],
            |_| Ok(true),
        ).unwrap_or(false);

        if !is_model_defined {
            return Ok(None);
        }

        let table_name = quote_identifier(resource_type);
        // 2. Query owner_id
        let owner: Option<String> = conn.query_row(
            &format!("SELECT owner_id FROM {} WHERE id = ?1", table_name),
            params![resource_id],
            |row| row.get(0),
        ).optional().map_err(|e| StateError::InternalError(e.to_string()))?;
        
        Ok(owner)
    }

    fn get_schema_version(&self) -> Result<Option<u64>, StateError> {
        let conn = self.conn.lock().unwrap();
        // Check table exists via sqlite_master
        let exists: bool = conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='__schema_version'",
            [],
            |_| Ok(true)
        ).unwrap_or(false);

        if !exists {
            return Ok(None);
        }

        let version_nested: Option<Option<u64>> = conn.query_row(
            "SELECT MAX(version) FROM __schema_version",
            [],
            |row| row.get(0)
        ).optional().map_err(|e| StateError::InternalError(e.to_string()))?;

        Ok(version_nested.flatten())
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

    // ========================================================================
    // Health Check Tests
    // ========================================================================

    #[test]
    fn test_ping_succeeds() {
        let state = create_state();
        assert!(state.ping().is_ok());
    }

    #[test]
    fn test_has_any_internal_user_empty_db() {
        let state = create_state();
        // Fresh DB with no migrations run - table doesn't exist
        let result = state.has_any_internal_user();
        assert!(result.is_ok());
        assert!(!result.unwrap()); // No users
    }

    #[test]
    fn test_has_any_internal_user_after_bootstrap() {
        let state = create_state();
        
        // Simulate migration creating the __internal_users table
        state.with_connection(|conn| {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS __internal_users (
                    id TEXT PRIMARY KEY,
                    external_subject TEXT NOT NULL,
                    roles TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                )",
                [],
            ).unwrap();
        });

        // Still no users
        assert!(!state.has_any_internal_user().unwrap());

        // Add a user
        state.with_connection(|conn| {
            conn.execute(
                "INSERT INTO __internal_users (id, external_subject, roles, status, created_at) 
                 VALUES ('user-1', 'admin@test.com', '[]', 'active', 0)",
                [],
            ).unwrap();
        });

        // Now has user
        assert!(state.has_any_internal_user().unwrap());
    }
}

