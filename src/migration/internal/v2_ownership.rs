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
        // Add owner_id to __models if not exists
        // SQLite doesn't support IF NOT EXISTS for columns easily in one statement, 
        // but adding a column that exists throws an error.
        // We can check or just try. State handles execute_ddl which expects plain SQL.
        
        // Since we are adding it as nullable initially to support existing rows?
        // Or user tables are empty?
        // Let's add as NULLABLE first? Or DEFAULT?
        // "Admin can manage schema" -> owned by admin?
        // Let's add as nullable for now to be safe, or default to 'system'?
        
        // Actually, we can just say "owner_id TEXT" and it defaults to NULL.
        // Then we can update existing rows if needed?
        // Existing models are kernel models (__models, etc). They don't have explicit entries in __models table (they are hardcoded).
        // The __models table contains USER models. If there are no user models yet, strict is fine.
        // If there are, they need an owner.
        
        state.execute_ddl("ALTER TABLE __models ADD COLUMN owner_id TEXT;")
             .map_err(|e| TransportError::Internal(e.to_string()))?;

        Ok(())
    }
}
