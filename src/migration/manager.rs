//! Migration Manager.

use std::time::{SystemTime, UNIX_EPOCH};
use crate::state::State;
use crate::transport::error::TransportError;
use super::InternalMigration;
use super::internal;

pub struct MigrationManager {
    migrations: Vec<Box<dyn InternalMigration>>,
}

impl MigrationManager {
    pub fn new() -> Self {
        Self {
            migrations: vec![
                Box::new(internal::V0Bootstrap),
                Box::new(internal::V1SchemaMeta),
                Box::new(internal::V2Ownership),
            ],
        }
    }

    pub fn run(&self, state: &mut dyn State) -> Result<(), TransportError> {
        // Simple approach for V0: Always try to apply bootstrap if not exists.
        // Bootstrap creates tables IF NOT EXISTS.
        // But for future, we need version check.
        // V0 creates `__schema_version`.
        // So we run V0 first.
        
        let pending = &self.migrations;
        
        for migration in pending {
            // In a real system:
            // 1. Get current version (catch error if table missing)
            // 2. If version < migration.version(), apply.
            // For now, V0 is safe to run repeatedly (IF NOT EXISTS).
            
            // Just apply.
            migration.apply(state)?;
            
            // Record version (Upsert?)
            // We need a way to insert into __schema_version.
            // State trait doesn't have "insert internal".
            // Use execute_ddl for INSERT too? (Since it's execution).
            // Yes.
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            let sql = format!(
                "INSERT OR REPLACE INTO __schema_version (version, applied_at) VALUES ({}, {});",
                migration.version(), now
            );
            state.execute_ddl(&sql)?;
        }

        Ok(())
    }
}
