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
                Box::new(internal::V3Auth),
                Box::new(internal::V4IndexOwnership),
            ],
        }
    }

    pub fn run(&self, state: &mut dyn State) -> Result<(), TransportError> {
        // 1. Get current Max Version (if table exists)
        let current_version = state.get_schema_version()
            .map_err(|e| TransportError::Internal(e.to_string()))?;

        // If None, it means table missing -> Run everything (start from 0)
        // If Some(v), run migrations with version > v
        
        let pending = &self.migrations;
        
        for migration in pending {
            if let Some(max_ver) = current_version {
                if migration.version() <= max_ver {
                    continue; // Skip already applied
                }
            }
            
            println!("Applying migration: {} (v{})", migration.name(), migration.version());

            // Just apply.
            migration.apply(state)?;
            
            // Record version (Upsert?)
            // We need a way to insert into __schema_version.
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            let sql = format!(
                "INSERT OR REPLACE INTO __schema_version (version, applied_at) VALUES ({}, {});",
                migration.version(), now
            );
            state.execute_ddl(&sql).map_err(|e| TransportError::Internal(e.to_string()))?;
        }
        Ok(())
    }
}
