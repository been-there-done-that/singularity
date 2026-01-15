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
        // Add owner_id to __models if not exists.
        // We can't use generic execute_ddl to check result easily without parsing error.
        // But we can check if column exists using State's internal connection? 
        // No, State trait is abstract.
        // But we added get_schema_version, implying we can add other checks?
        // Or just Try-Catch using the error string?
        
        let result = state.execute_ddl("ALTER TABLE __models ADD COLUMN owner_id TEXT;");
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("duplicate column name") {
                    println!("Column owner_id already exists, skipping.");
                    Ok(())
                } else {
                    Err(TransportError::Internal(msg))
                }
            }
        }
    }
}
