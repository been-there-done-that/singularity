use crate::migration::InternalMigration;
use crate::state::State;
use crate::transport::error::TransportError;

/// V2: Ownership.
/// Adds `owner_id` to `__models`.
pub struct V2Ownership;

impl InternalMigration for V2Ownership {
    fn version(&self) -> u64 {
        2
    }

    fn name(&self) -> &str {
        "add_ownership_column"
    }

    fn apply(&self, state: &mut dyn State) -> Result<(), TransportError> {
        // Add owner_id to __models
        let _ = state.execute_ddl("ALTER TABLE __models ADD COLUMN owner_id TEXT;");
        
        // Add owner_id to __fields
        let _ = state.execute_ddl("ALTER TABLE __fields ADD COLUMN owner_id TEXT;");

        Ok(())
    }
}
