use crate::migration::InternalMigration;
use crate::state::State;
use crate::transport::error::TransportError;

/// V0: Bootstrap.
/// Creates `__schema_version`, `__models`, `__internal_users`.
pub struct V0Bootstrap;

impl InternalMigration for V0Bootstrap {
    fn version(&self) -> u64 {
        0
    }

    fn name(&self) -> &str {
        "bootstrap_system_tables"
    }

    fn apply(&self, state: &mut dyn State) -> Result<(), TransportError> {
        // We need raw SQL execution here.
        // The State trait needs to expose a way to run internal DDL.
        // Assuming State has/will have `execute_ddl` or similar.
        
        // __schema_version
        state.execute_ddl("
            CREATE TABLE IF NOT EXISTS __schema_version (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );
        ").map_err(|e| TransportError::Internal(e.to_string()))?;

        // __models (Registry of user tables)
        state.execute_ddl("
            CREATE TABLE IF NOT EXISTS __models (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                schema_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
        ").map_err(|e| TransportError::Internal(e.to_string()))?;

        // __internal_users (Anchor for identity)
        state.execute_ddl("
            CREATE TABLE IF NOT EXISTS __internal_users (
                id TEXT PRIMARY KEY,
                external_subject TEXT NOT NULL UNIQUE,
                roles TEXT NOT NULL, -- JSON array
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
        ").map_err(|e| TransportError::Internal(e.to_string()))?;

        Ok(())
    }
}
