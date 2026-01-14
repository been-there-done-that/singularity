use crate::migration::InternalMigration;
use crate::state::State;
use crate::transport::error::TransportError;

/// V1: Schema Meta-Model.
/// Creates `__models`, `__fields`, `__folders`, `__indexes`.
pub struct V1SchemaMeta;

impl InternalMigration for V1SchemaMeta {
    fn version(&self) -> u64 {
        1
    }

    fn name(&self) -> &str {
        "init_schema_meta_tables"
    }

    fn apply(&self, state: &mut dyn State) -> Result<(), TransportError> {
        // __models - Registry of user tables
        state.execute_ddl("
            CREATE TABLE IF NOT EXISTS __models (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                namespace TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(namespace, name)
            );
        ").map_err(|e| TransportError::Internal(e.to_string()))?;

        // __fields - Columns per model
        state.execute_ddl("
            CREATE TABLE IF NOT EXISTS __fields (
                id TEXT PRIMARY KEY,
                model_id TEXT NOT NULL,
                name TEXT NOT NULL,
                field_type TEXT NOT NULL, -- JSON serialized FieldType
                required INTEGER NOT NULL DEFAULT 0,
                unique_flag INTEGER NOT NULL DEFAULT 0,
                default_val TEXT, -- JSON serialized default
                created_at INTEGER NOT NULL,
                FOREIGN KEY(model_id) REFERENCES __models(id) ON DELETE CASCADE,
                UNIQUE(model_id, name)
            );
        ").map_err(|e| TransportError::Internal(e.to_string()))?;
        
        // __folders - Logical grouping
        state.execute_ddl("
            CREATE TABLE IF NOT EXISTS __folders (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                parent_id TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(parent_id) REFERENCES __folders(id) ON DELETE CASCADE
            );
        ").map_err(|e| TransportError::Internal(e.to_string()))?;
        
        // __indexes (Future use)
        state.execute_ddl("
            CREATE TABLE IF NOT EXISTS __indexes (
                id TEXT PRIMARY KEY,
                model_id TEXT NOT NULL,
                name TEXT NOT NULL,
                fields TEXT NOT NULL, -- JSON array of field IDs
                unique_flag INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(model_id) REFERENCES __models(id) ON DELETE CASCADE
            );
        ").map_err(|e| TransportError::Internal(e.to_string()))?;

        Ok(())
    }
}
