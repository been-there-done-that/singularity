//! V5: Row-Level Security (RLS) Tables.
//!
//! Adds:
//! - `default_row_scope` column to `__models`
//! - `__access_profiles` table
//! - `__row_scopes` table

use crate::migration::InternalMigration;
use crate::state::State;
use crate::transport::error::TransportError;

/// V5: Row-Level Security schema.
pub struct V5Rls;

impl InternalMigration for V5Rls {
    fn version(&self) -> u64 {
        5
    }

    fn name(&self) -> &str {
        "rls_tables"
    }

    fn apply(&self, state: &mut dyn State) -> Result<(), TransportError> {
        // 1. Add model default scope column
        state.execute_ddl(
            "ALTER TABLE __models ADD COLUMN default_row_scope TEXT 
             CHECK(default_row_scope IN ('owner', 'all') OR default_row_scope IS NULL);"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // 2. Create access profiles table
        state.execute_ddl(
            "CREATE TABLE IF NOT EXISTS __access_profiles (
                id TEXT PRIMARY KEY,
                model_id TEXT NOT NULL,
                principal_type TEXT NOT NULL CHECK(principal_type IN ('role', 'group', 'user')),
                principal_id TEXT NOT NULL,
                allow_query INTEGER DEFAULT 0,
                allow_insert INTEGER DEFAULT 0,
                allow_update INTEGER DEFAULT 0,
                allow_delete INTEGER DEFAULT 0,
                priority INTEGER DEFAULT 0 CHECK(priority >= 0),
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                owner_id TEXT,
                UNIQUE(model_id, principal_type, principal_id)
            );"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // 3. Create row scopes table
        state.execute_ddl(
            "CREATE TABLE IF NOT EXISTS __row_scopes (
                id TEXT PRIMARY KEY,
                access_profile_id TEXT NOT NULL,
                opcode TEXT NOT NULL CHECK(opcode IN ('query', 'update', 'delete')),
                scope_type TEXT NOT NULL CHECK(scope_type IN ('owner', 'all', 'predicate', 'deny')),
                predicate_dsl TEXT,
                UNIQUE(access_profile_id, opcode),
                FOREIGN KEY (access_profile_id) REFERENCES __access_profiles(id) ON DELETE CASCADE
            );"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // 4. Create index for efficient lookup
        state.execute_ddl(
            "CREATE INDEX IF NOT EXISTS idx_access_profiles_lookup 
             ON __access_profiles(model_id, principal_type, principal_id);"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        Ok(())
    }
}
