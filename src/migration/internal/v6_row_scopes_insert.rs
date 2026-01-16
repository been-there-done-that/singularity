//! V6: Fix row_scopes opcode constraint.
//!
//! Adds 'insert' to the opcode CHECK constraint in `__row_scopes`.
//! SQLite doesn't support ALTER CONSTRAINT, so we recreate the table.

use crate::migration::InternalMigration;
use crate::state::State;
use crate::transport::error::TransportError;

/// V6: Add 'insert' opcode to row_scopes.
pub struct V6RowScopesInsert;

impl InternalMigration for V6RowScopesInsert {
    fn version(&self) -> u64 {
        6
    }

    fn name(&self) -> &str {
        "row_scopes_insert_opcode"
    }

    fn apply(&self, state: &mut dyn State) -> Result<(), TransportError> {
        // SQLite doesn't support modifying CHECK constraints, so we need to:
        // 1. Create new table with correct constraint
        // 2. Copy data
        // 3. Drop old table
        // 4. Rename new table

        // 1. Create new table
        state.execute_ddl(
            "CREATE TABLE IF NOT EXISTS __row_scopes_new (
                id TEXT PRIMARY KEY,
                access_profile_id TEXT NOT NULL,
                opcode TEXT NOT NULL CHECK(opcode IN ('query', 'insert', 'update', 'delete')),
                scope_type TEXT NOT NULL CHECK(scope_type IN ('owner', 'all', 'predicate', 'deny')),
                predicate_dsl TEXT,
                UNIQUE(access_profile_id, opcode),
                FOREIGN KEY (access_profile_id) REFERENCES __access_profiles(id) ON DELETE CASCADE
            );"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // 2. Copy data
        state.execute_ddl(
            "INSERT INTO __row_scopes_new SELECT * FROM __row_scopes;"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // 3. Drop old table
        state.execute_ddl(
            "DROP TABLE __row_scopes;"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // 4. Rename new table
        state.execute_ddl(
            "ALTER TABLE __row_scopes_new RENAME TO __row_scopes;"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        Ok(())
    }
}
