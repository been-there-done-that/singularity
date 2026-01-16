//! V7: Operation Constraints storage.
//!
//! Adds `__operation_constraints` table for mutation validation rules.

use crate::migration::InternalMigration;
use crate::state::State;
use crate::transport::error::TransportError;

/// V7: Create operation_constraints table.
pub struct V7OperationConstraints;

impl InternalMigration for V7OperationConstraints {
    fn version(&self) -> u64 {
        7
    }

    fn name(&self) -> &str {
        "operation_constraints"
    }

    fn apply(&self, state: &mut dyn State) -> Result<(), TransportError> {
        // 1. Create operation_constraints table
        state.execute_ddl(
            "CREATE TABLE IF NOT EXISTS __operation_constraints (
                id TEXT PRIMARY KEY,
                model_id TEXT NOT NULL,
                action TEXT NOT NULL CHECK(action IN ('create', 'update')),
                constraint_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        // 2. Create index for efficient lookup by model + action
        state.execute_ddl(
            "CREATE INDEX IF NOT EXISTS idx_operation_constraints_model_action 
             ON __operation_constraints(model_id, action);"
        ).map_err(|e| TransportError::Internal(e.to_string()))?;

        Ok(())
    }
}
