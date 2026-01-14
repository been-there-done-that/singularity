//! Internal Migration System.
//!
//! Manages the kernel's own schema evolution.
//!
//! # Layers
//! - `trait InternalMigration`: Defines a versioned change.
//! - `manager`: Orchestrates application at startup.
//! - `internal`: Concrete migrations (v0, v1, etc).

use crate::state::State;
use crate::transport::error::TransportError;

pub mod manager;
pub mod internal;

/// A single atomic migration.
pub trait InternalMigration: Send + Sync {
    /// Version number (monotonic).
    fn version(&self) -> u64;

    /// Description.
    fn name(&self) -> &str;

    /// Apply the migration to the state.
    fn apply(&self, state: &mut dyn State) -> Result<(), TransportError>;
}
