use crate::migration::InternalMigration;
use crate::state::State;
use crate::transport::error::TransportError;

/// V4: Index Ownership.
/// Adds `owner_id` to `__indexes`.
pub struct V4IndexOwnership;

impl InternalMigration for V4IndexOwnership {
    fn version(&self) -> u64 {
        4
    }

    fn name(&self) -> &str {
        "add_index_ownership"
    }

    fn apply(&self, state: &mut dyn State) -> Result<(), TransportError> {
        state.execute_ddl("ALTER TABLE __indexes ADD COLUMN owner_id TEXT;")
            .map_err(|e| TransportError::Internal(e.to_string()))?;
        Ok(())
    }
}
